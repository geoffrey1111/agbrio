//! Fixed official binary, bounded owned stdio, and observations installed at spawn.
use super::native::{NativeFactory, NativeSession};
use crate::{
    codex::{
        adapter::{CandidateCleanup, CodexAdapter},
        critical::{channel, CriticalReceiver},
        preparation::PreparationGuard,
    },
    events::NullEventSink,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

pub const NATIVE_SHA256: &str = "be96b992178b1e467c225800da0d65f2c86d5eba1ef0b14632f65db381cbdfde";
#[derive(Clone)]
pub struct NativeLaunch {
    pub executable: PathBuf,
    pub codex_home: PathBuf,
    pub config_sha256: String,
    pub overrides: Vec<String>,
}
impl NativeLaunch {
    /// Adds the fixed workspace-write confinement for one already-sealed
    /// dispatch root. This is per child: a process can never inherit another
    /// relay's writable root from the shared baseline.
    pub fn confined_to_workspace_root(&self, root: &Path) -> Result<Self, String> {
        let canonical = std::fs::canonicalize(root).map_err(|_| "ROOT_CHANGED")?;
        let root = canonical.to_str().ok_or("ROOT_CHANGED")?;
        let roots = serde_json::to_string(&vec![root]).map_err(|_| "ROOT_CHANGED")?;
        let mut launch = self.clone();
        launch.overrides.extend([
            "sandbox_workspace_write.exclude_tmpdir_env_var=true".into(),
            "sandbox_workspace_write.exclude_slash_tmp=true".into(),
            format!("sandbox_workspace_write.writable_roots={roots}"),
        ]);
        Ok(launch)
    }
    pub fn verify(&self) -> Result<(), String> {
        if !self.executable.is_absolute()
            || !self.codex_home.is_absolute()
            || !self.codex_home.is_dir()
        {
            return Err("NATIVE_CONFIG_INVALID".into());
        }
        verify_config(&self.codex_home.join("config.toml"), &self.config_sha256)?;
        let mut file = File::open(&self.executable).map_err(|_| "NATIVE_VERSION_MISMATCH")?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|_| "NATIVE_VERSION_MISMATCH")?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != NATIVE_SHA256 {
            return Err("NATIVE_VERSION_MISMATCH".into());
        }
        Ok(())
    }
    pub fn open_owned(&self) -> Result<OwnedNative, String> {
        self.open_owned_at_epoch(uuid::Uuid::new_v4().to_string())
    }
    pub fn open_owned_at_epoch(&self, epoch: String) -> Result<OwnedNative, String> {
        let mut owned = self.open_uninitialized_at_epoch(epoch)?;
        if let Err(e) = owned.initialize() {
            let _ = owned.close();
            return Err(e);
        }
        Ok(owned)
    }
    pub fn open_uninitialized_at_epoch(&self, epoch: String) -> Result<OwnedNative, String> {
        self.verify()?;
        let guard = PreparationGuard::default();
        let (sender, events) = channel(guard.clone());
        let mut command = Command::new(&self.executable);
        command.env("CODEX_HOME", &self.codex_home);
        for override_value in &self.overrides {
            command.arg("-c").arg(override_value);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        // The pinned 0.154.0 executable deliberately runs without
        // `--strict-config`. A shared, forward-compatible Codex config can
        // contain newer feature keys that this fixed executable correctly
        // ignores, while `--strict-config` would make that unrelated key a
        // startup failure before Router can apply and verify its own bounded
        // launch overrides. The raw source digest and the complete ordered
        // Router override list remain checked before spawn and before every
        // RPC; this is not a permissive Router policy mode.
        let adapter = CodexAdapter::start_guarded_at_epoch(
            Arc::new(NullEventSink),
            Arc::new(move |event| sender.observe(event)),
            command,
            guard.clone(),
            epoch,
        )?;
        Ok(OwnedNative {
            adapter,
            guard,
            events,
            sequence: 0,
            closed: false,
            config_path: self.codex_home.join("config.toml"),
            config_sha256: self.config_sha256.clone(),
            counts: std::collections::BTreeMap::new(),
            cleanup: CandidateCleanup::default(),
        })
    }
}

impl NativeFactory for NativeLaunch {
    fn open(&mut self) -> Result<Box<dyn NativeSession>, String> {
        Ok(Box::new(self.open_owned()?))
    }
}
pub struct OwnedNative {
    pub(crate) adapter: CodexAdapter,
    pub(crate) guard: PreparationGuard,
    events: CriticalReceiver,
    sequence: i64,
    closed: bool,
    pub(crate) config_path: PathBuf,
    pub(crate) config_sha256: String,
    counts: std::collections::BTreeMap<String, u64>,
    cleanup: CandidateCleanup,
}
impl OwnedNative {
    pub fn observation_summary(&self) -> Value {
        json!({"adapter_epoch":self.epoch(),"rpc_attempts":self.counts,"observed_turn_starts":self.guard.turn_starts(),"observation_failed":self.guard.check().is_err(),"exited_and_drained":self.exited_and_drained(),"exceptional_cleanup":{"clear_attempted":self.cleanup.clear_attempted,"clear_acknowledged":self.cleanup.clear_acknowledged,"interrupt_attempted":self.cleanup.interrupt_attempted,"interrupt_acknowledged":self.cleanup.interrupt_acknowledged},"billing_evidence":"NOT_OBSERVED"})
    }
    pub(crate) fn count_turn_attempt(&mut self) {
        *self.counts.entry("turn/start".into()).or_default() += 1;
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        self.request("initialize",json!({"clientInfo":{"name":"ai_work_router_desktop","version":"0.1.0"},"capabilities":{"experimentalApi":true,"requestAttestation":false}}))?;
        self.adapter.notify("initialized", Value::Null)
    }

    pub fn epoch(&self) -> &str {
        self.adapter.epoch()
    }
    pub fn exited_and_drained(&self) -> bool {
        self.adapter.exited_and_drained()
    }
    pub fn respond(&mut self, reply: crate::codex::requests::NativeReply) -> Result<(), String> {
        self.adapter.respond_verified(reply)
    }
    pub fn interrupt_exact(&mut self, thread: &str, turn: &str) -> Result<(), String> {
        if self.guard.observed_turn() != Some((thread.into(), turn.into())) {
            return Err("FORBIDDEN".into());
        }
        self.request("turn/interrupt", json!({"threadId":thread,"turnId":turn}))?;
        Ok(())
    }

    /// Native events are bounded and already safety-observed on the reader.
    pub fn next_event(&mut self) -> Result<Option<(i64, Value)>, String> {
        self.guard.check()?;
        match self.events.try_next()? {
            Some(event) => {
                self.sequence = self.sequence.checked_add(1).ok_or("OBSERVATION_FAILED")?;
                Ok(Some((self.sequence, event)))
            }
            None => Ok(None),
        }
    }
}
impl NativeSession for OwnedNative {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if self.closed {
            return Err("NATIVE_CLOSED".into());
        }
        verify_config(&self.config_path, &self.config_sha256)?;
        *self.counts.entry(method.into()).or_default() += 1;
        self.adapter
            .request_with_timeout(method, params, Duration::from_secs(10))
    }
    fn begin_exact_history_recovery(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), String> {
        if self.closed {
            return Err("NATIVE_CLOSED".into());
        }
        verify_config(&self.config_path, &self.config_sha256)?;
        self.adapter
            .begin_exact_history_recovery(thread_id, turn_id)
    }
    fn settle(&mut self, window: Duration) -> Result<(), String> {
        let until = Instant::now() + window;
        loop {
            self.guard.check()?;
            while self.next_event()?.is_some() {}
            if Instant::now() >= until {
                return self.guard.check();
            }
            std::thread::sleep(
                Duration::from_millis(25).min(until.saturating_duration_since(Instant::now())),
            );
        }
    }
    fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return self.guard.check();
        }
        self.adapter.shutdown_guarded()?;
        loop {
            match self.events.try_next() {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(code) if code == "OBSERVATION_CLOSED" => break,
                Err(code) => return Err(code),
            }
        }
        self.guard.check()?;
        self.closed = true;
        Ok(())
    }
    fn cleanup_candidate(&mut self) -> CandidateCleanup {
        let result = self.adapter.cleanup_owned_candidate();
        self.cleanup.clear_attempted |= result.clear_attempted;
        self.cleanup.clear_acknowledged |= result.clear_acknowledged;
        self.cleanup.interrupt_attempted |= result.interrupt_attempted;
        self.cleanup.interrupt_acknowledged |= result.interrupt_acknowledged;
        result
    }
}

pub(crate) fn verify_config(path: &std::path::Path, expected: &str) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "NATIVE_CONFIG_INVALID")?;
    if metadata.len() > 1048576 || metadata.file_type().is_symlink() {
        return Err("NATIVE_CONFIG_INVALID".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "NATIVE_CONFIG_INVALID")?;
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err("POLICY_CHANGED".into());
    }
    Ok(())
}
