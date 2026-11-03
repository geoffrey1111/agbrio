//! Current-user, marked launchd sampler. Only explicit desktop Save mutates it.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
extern "C" {
    fn geteuid() -> u32;
}
fn uid() -> u32 {
    unsafe { geteuid() }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Owner {
    schema: String,
    owner_uid: u32,
    host_id: String,
    label: String,
    binary_path: String,
    binary_sha256: String,
    prepared: bool,
    is_test: bool,
    plist_sha256: Option<String>,
}
pub(crate) fn root() -> Option<PathBuf> {
    router_core::platform::local_data_base()
        .ok()
        .map(|p| p.join("AIWorkRouter/resource-monitor"))
}
fn linked(path: &Path) -> Result<(), String> {
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("RESOURCE_MONITOR_PATH_INVALID".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("RESOURCE_MONITOR_STORAGE_UNAVAILABLE".into()),
        }
    }
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn helper() -> Result<PathBuf, String> {
    let current = std::env::current_exe().map_err(|_| "RESOURCE_MONITOR_NOT_PACKAGED")?;
    let bundled = current
        .parent()
        .and_then(Path::parent)
        .ok_or("RESOURCE_MONITOR_NOT_PACKAGED")?
        .join("Resources/macos-platform/mac-resource-sampler");
    if bundled.is_file() {
        return fs::canonicalize(bundled).map_err(|_| "RESOURCE_MONITOR_NOT_PACKAGED".into());
    }
    #[cfg(debug_assertions)]
    {
        let development = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/macos-platform/mac-resource-sampler");
        if development.is_file() {
            return fs::canonicalize(development)
                .map_err(|_| "RESOURCE_MONITOR_NOT_PACKAGED".into());
        }
    }
    Err("RESOURCE_MONITOR_NOT_PACKAGED".into())
}
fn private_bytes(path: &Path) -> Result<Vec<u8>, String> {
    linked(path)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(0x100)
        .open(path)
        .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
    let m = file
        .metadata()
        .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
    if !m.is_file()
        || m.uid() != uid()
        || m.mode() & 0o077 != 0
        || m.nlink() != 1
        || m.len() > 65536
    {
        return Err("RESOURCE_MONITOR_STORAGE_UNTRUSTED".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
    if bytes.len() > 65536 {
        return Err("RESOURCE_MONITOR_STORAGE_UNTRUSTED".into());
    }
    Ok(bytes)
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    linked(path)?;
    if path.exists() {
        let _ = private_bytes(path)?;
    }
    let parent = path
        .parent()
        .ok_or("RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
    let temporary = parent.join(format!(".agbrio-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "RESOURCE_MONITOR_WRITE_UNCONFIRMED")?;
        fs::rename(&temporary, path).map_err(|_| "RESOURCE_MONITOR_WRITE_UNCONFIRMED".into())
    })();
    let _ = fs::remove_file(temporary);
    result
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    atomic(
        path,
        &serde_json::to_vec(value).map_err(|_| "RESOURCE_MONITOR_SETTINGS_INVALID")?,
    )
}
fn owner(root: &Path) -> Result<Option<Owner>, String> {
    linked(root)?;
    if !root.exists() {
        return Ok(None);
    }
    let m = fs::symlink_metadata(root).map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
    if !m.is_dir() || m.uid() != uid() || m.mode() & 0o777 != 0o700 {
        return Err("RESOURCE_MONITOR_STORAGE_UNTRUSTED".into());
    }
    let marker = root.join("owner.json");
    if !marker.exists() {
        return if fs::read_dir(root)
            .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?
            .next()
            .is_none()
        {
            Ok(None)
        } else {
            Err("RESOURCE_MONITOR_FOREIGN_PRESERVED".into())
        };
    }
    let value: Owner = serde_json::from_slice(&private_bytes(&marker)?)
        .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNTRUSTED")?;
    if value.schema != "AGBRIO_MAC_RESOURCE_OWNER_V1"
        || value.owner_uid != uid()
        || !uuid::Uuid::parse_str(&value.host_id).is_ok_and(|id| id.to_string() == value.host_id)
        || value.label
            != format!(
                "com.geoffrey.aiworkrouter.resources.{}",
                value.host_id.replace('-', "")
            )
        || value.binary_sha256.len() != 64
    {
        return Err("RESOURCE_MONITOR_STORAGE_UNTRUSTED".into());
    }
    Ok(Some(value))
}
pub(crate) fn host_id() -> Option<String> {
    let root = root()?;
    let owner = owner(&root).ok()??;
    (!owner.prepared).then_some(owner.host_id)
}
pub(crate) fn available() -> bool {
    helper().is_ok() && host_id().is_some()
}
fn native(args: &[String]) -> Result<Value, String> {
    let mut command = Command::new(helper()?);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", router_core::platform::user_home()?)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| "RESOURCE_MONITOR_UNAVAILABLE")?;
    let stdout = child.stdout.take().ok_or("RESOURCE_MONITOR_UNAVAILABLE")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.take(131073).read_to_end(&mut bytes);
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break exit,
            Ok(None) if started.elapsed() < Duration::from_secs(25) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("RESOURCE_MONITOR_READ_TIMEOUT".into());
            }
        }
    };
    let bytes = reader.join().map_err(|_| "RESOURCE_MONITOR_UNAVAILABLE")?;
    if bytes.len() > 131072 {
        return Err("RESOURCE_MONITOR_RESPONSE_INVALID".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "RESOURCE_MONITOR_RESPONSE_INVALID")?;
    if !status.success() {
        let code = value["errorCode"]
            .as_str()
            .filter(|s| {
                s.starts_with("RESOURCE_MONITOR_")
                    && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
            })
            .unwrap_or("RESOURCE_MONITOR_UNAVAILABLE");
        return Err(code.into());
    }
    Ok(value)
}
fn identity(pid: u32) -> Result<Value, String> {
    native(&["identity".into(), pid.to_string()])
}
fn domain() -> String {
    format!("user/{}", uid())
}
struct Job {
    pid: Option<u32>,
    program: String,
    plist: String,
}
fn job(label: &str) -> Result<Option<Job>, String> {
    // Query only the exact marked service, in an explicit user domain. Keep
    // only pid/program/plist projections; never return/log environment/argv.
    let output = Command::new("/bin/launchctl")
        .args(["print", &format!("{}/{label}", domain())])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "RESOURCE_MONITOR_LAUNCHD_UNAVAILABLE")?;
    if output.stdout.len() > 1024 * 1024 || output.stderr.len() > 65536 {
        return Err("RESOURCE_MONITOR_LAUNCHD_UNAVAILABLE".into());
    }
    if !output.status.success() {
        return if String::from_utf8_lossy(&output.stderr).contains("Could not find service") {
            Ok(None)
        } else {
            Err("RESOURCE_MONITOR_LAUNCHD_UNAVAILABLE".into())
        };
    }
    let mut pid = None;
    let (mut program, mut plist) = (None, None);
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("pid = ") {
            pid = Some(
                value
                    .parse()
                    .map_err(|_| "RESOURCE_MONITOR_LAUNCHD_UNAVAILABLE")?,
            );
        } else if let Some(value) = line.strip_prefix("program = ") {
            program = Some(value.trim_matches('"').to_owned());
        } else if let Some(value) = line.strip_prefix("path = ") {
            plist = Some(value.trim_matches('"').to_owned());
        }
    }
    Ok(Some(Job {
        pid,
        program: program.ok_or("RESOURCE_MONITOR_LAUNCHD_UNAVAILABLE")?,
        plist: plist.ok_or("RESOURCE_MONITOR_LAUNCHD_UNAVAILABLE")?,
    }))
}
fn check_job(root: &Path, owner: &Owner) -> Result<Option<Job>, String> {
    let value = job(&owner.label)?;
    if value.as_ref().is_some_and(|j| {
        j.program != owner.binary_path || j.plist != root.join("collector.plist").to_string_lossy()
    }) {
        return Err("RESOURCE_MONITOR_FOREIGN_PRESERVED".into());
    }
    if let Some(pid) = value.as_ref().and_then(|j| j.pid) {
        let actual = identity(pid)?;
        if actual["state"] == "EXITED" {
            return Ok(value);
        }
        if actual["ownerUid"] != uid()
            || actual["image"].as_str() != Some(owner.binary_path.as_str())
        {
            return Err("RESOURCE_MONITOR_JOB_IDENTITY_UNCONFIRMED".into());
        }
    }
    Ok(value)
}
fn pending_settled(root: &Path) -> Result<bool, String> {
    let file = root.join("apply-pending.json");
    if !file.exists() {
        return Ok(true);
    }
    let pending: Value = serde_json::from_slice(&private_bytes(&file)?)
        .map_err(|_| "RESOURCE_MONITOR_APPLY_UNCONFIRMED")?;
    if pending["terminal"] == true {
        return Ok(true);
    }
    let child = &pending["child"];
    let Some(pid) = child["pid"].as_u64().and_then(|p| u32::try_from(p).ok()) else {
        return Ok(false);
    };
    let actual = identity(pid)?;
    Ok(actual["state"] == "EXITED"
        || actual["ownerUid"] != child["ownerUid"]
        || actual["created"] != child["created"]
        || actual["image"] != child["image"])
}
fn snapshot(root: &Path) -> Result<Value, String> {
    let owner = owner(root)?;
    let _read_guard = if root.join("collector.lock").exists() {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(0x100)
            .open(root.join("collector.lock"))
            .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNTRUSTED")?;
        let m = lock
            .metadata()
            .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNTRUSTED")?;
        if !m.is_file() || m.uid() != uid() || m.mode() & 0o777 != 0o600 || m.nlink() != 1 {
            return Err("RESOURCE_MONITOR_STORAGE_UNTRUSTED".into());
        }
        lock.try_lock()
            .map_err(|_| "RESOURCE_MONITOR_APPLY_UNCONFIRMED")?;
        Some(lock)
    } else {
        None
    };
    if owner.is_some() && !pending_settled(root)? {
        return Err("RESOURCE_MONITOR_APPLY_UNCONFIRMED".into());
    }
    let mut value = native(&["snapshot".into(), root.to_string_lossy().into_owned()])?;
    if let Some(owner) = owner {
        let binary = helper()?;
        let active = check_job(root, &owner)?.is_some();
        let changed = hash(&fs::read(&binary).map_err(|_| "RESOURCE_MONITOR_NOT_PACKAGED")?)
            != owner.binary_sha256
            || binary.to_str() != Some(owner.binary_path.as_str());
        if !owner.prepared {
            let bytes = private_bytes(&root.join("collector.plist"))?;
            if owner.plist_sha256.as_deref() != Some(hash(&bytes).as_str())
                || bytes
                    != plist(
                        root,
                        &owner,
                        &value["config"],
                        Path::new(&owner.binary_path),
                    )?
            {
                return Err("RESOURCE_MONITOR_STATE_UNCONFIRMED".into());
            }
        }
        value["running"] = json!(
            active
                && value["config"]["Enabled"] == true
                && !changed
                && !owner.prepared
                && value["selectionUnavailable"] != true
        );
        value["needsSave"] = json!(value["needsSave"] == true || changed || owner.prepared);
    }
    Ok(value)
}
pub(crate) fn read() -> Result<Value, String> {
    snapshot(&root().ok_or("RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?)
}
fn validate(config: &Value, drives: &Value) -> Result<(), String> {
    let number = |key: &str, min: f64, max: f64| {
        config[key]
            .as_f64()
            .is_some_and(|v| v.is_finite() && (min..=max).contains(&v))
    };
    if config["SchemaVersion"] != 3
        || config["CommitPercent"].as_f64() != Some(0.0)
        || !config["Enabled"].is_boolean()
        || !number("AvailableMemoryPercent", 1.0, 95.0)
        || !number("SampleSeconds", 60.0, 3600.0)
        || !number("RepeatMinutes", 5.0, 1440.0)
    {
        return Err("RESOURCE_MONITOR_SETTINGS_INVALID".into());
    }
    let rows = config["Disks"]
        .as_array()
        .filter(|r| r.len() <= 64)
        .ok_or("RESOURCE_MONITOR_SETTINGS_INVALID")?;
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let id = row["Drive"]
            .as_str()
            .ok_or("RESOURCE_MONITOR_SETTINGS_INVALID")?;
        if !uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id)
            || !seen.insert(id)
            || !row["Enabled"].is_boolean()
            || !row["FreePercent"]
                .as_f64()
                .is_some_and(|v| v.is_finite() && (0.1..=95.0).contains(&v))
        {
            return Err("RESOURCE_MONITOR_SETTINGS_INVALID".into());
        }
        if row["Enabled"] == true
            && !drives
                .as_array()
                .is_some_and(|r| r.iter().any(|d| d["drive"] == id))
        {
            return Err("RESOURCE_MONITOR_DRIVE_UNAVAILABLE".into());
        }
    }
    Ok(())
}
fn plist(root: &Path, owner: &Owner, config: &Value, binary: &Path) -> Result<Vec<u8>, String> {
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let text=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- Agbrio-owned-resource-v1:{} -->\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>sample</string><string>{}</string></array><key>StartInterval</key><integer>{}</integer><key>RunAtLoad</key><false/><key>KeepAlive</key><false/><key>ProcessType</key><string>Background</string></dict></plist>\n",owner.host_id,owner.label,escape(binary.to_str().ok_or("RESOURCE_MONITOR_PATH_INVALID")?),escape(root.to_str().ok_or("RESOURCE_MONITOR_PATH_INVALID")?),config["SampleSeconds"].as_u64().ok_or("RESOURCE_MONITOR_SETTINGS_INVALID")?);
    Ok(text.into_bytes())
}
enum ControlError {
    Failed,
    Unknown(Child),
}
#[cfg(test)]
std::thread_local! {static FAIL_BOOTSTRAP:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
fn control(root: &Path, id: &str, args: &[String]) -> Result<(), ControlError> {
    #[cfg(test)]
    if args[0] == "bootstrap" && FAIL_BOOTSTRAP.with(|flag| flag.replace(false)) {
        return Err(ControlError::Failed);
    }
    // Write intent before dispatch. If the Host dies before native identity is
    // checkpointed, inspection stays UNCONFIRMED rather than replaying a write.
    write_json(
        &root.join("apply-pending.json"),
        &json!({"requestId":id,"step":args[0],"child":null}),
    )
    .map_err(|_| ControlError::Failed)?;
    let mut child = Command::new("/bin/launchctl")
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ControlError::Failed)?;
    if let Ok(Some(status)) = child.try_wait() {
        let _ = write_json(
            &root.join("apply-pending.json"),
            &json!({"requestId":id,"step":args[0],"terminal":true}),
        );
        return if status.success() {
            Ok(())
        } else {
            Err(ControlError::Failed)
        };
    }
    if let Ok(actual) = identity(child.id()) {
        let _ = write_json(
            &root.join("apply-pending.json"),
            &json!({"requestId":id,"step":args[0],"child":actual}),
        );
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = write_json(
                    &root.join("apply-pending.json"),
                    &json!({"requestId":id,"step":args[0],"terminal":true}),
                );
                return if status.success() {
                    Ok(())
                } else {
                    Err(ControlError::Failed)
                };
            }
            Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => return Err(ControlError::Unknown(child)),
        }
    }
}
fn restore(path: &Path, bytes: &Option<Vec<u8>>) -> Result<(), String> {
    if let Some(bytes) = bytes {
        atomic(path, bytes)
    } else {
        if path.exists() {
            let _ = private_bytes(path)?;
            fs::remove_file(path).map_err(|_| "RESOURCE_MONITOR_ROLLBACK_UNCONFIRMED")?;
        }
        Ok(())
    }
}
fn apply_at(root: &Path, config: Value, is_test: bool) -> Result<Value, String> {
    let before = snapshot(root)?;
    validate(&config, &before["drives"])?;
    let binary = helper()?;
    let image = hash(&fs::read(&binary).map_err(|_| "RESOURCE_MONITOR_NOT_PACKAGED")?);
    let mut meta = match owner(root)? {
        Some(meta) => meta,
        None => {
            linked(root)?;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(root)
                .map_err(|_| "RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?;
            let host = uuid::Uuid::new_v4().to_string();
            let meta = Owner {
                schema: "AGBRIO_MAC_RESOURCE_OWNER_V1".into(),
                owner_uid: uid(),
                label: format!(
                    "com.geoffrey.aiworkrouter.resources.{}",
                    host.replace('-', "")
                ),
                host_id: host,
                binary_path: binary.to_string_lossy().into_owned(),
                binary_sha256: image.clone(),
                prepared: true,
                is_test,
                plist_sha256: None,
            };
            write_json(&root.join("owner.json"), &meta)?;
            write_json(&root.join("settings.json"), &before["config"])?;
            meta
        }
    };
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(0x100)
        .open(root.join("collector.lock"))
        .map_err(|_| "RESOURCE_MONITOR_BUSY")?;
    let m = lock.metadata().map_err(|_| "RESOURCE_MONITOR_BUSY")?;
    if !m.is_file() || m.uid() != uid() || m.mode() & 0o777 != 0o600 || m.nlink() != 1 {
        return Err("RESOURCE_MONITOR_STORAGE_UNTRUSTED".into());
    }
    lock.try_lock().map_err(|_| "RESOURCE_MONITOR_BUSY")?;
    if !pending_settled(root)? {
        return Err("RESOURCE_MONITOR_APPLY_UNCONFIRMED".into());
    }
    let _ = fs::remove_file(root.join("apply-pending.json"));
    let file = root.join("collector.plist");
    let old_plist = if file.exists() {
        let bytes = private_bytes(&file)?;
        if meta.plist_sha256.as_deref() != Some(hash(&bytes).as_str()) {
            return Err("RESOURCE_MONITOR_FOREIGN_PRESERVED".into());
        }
        Some(bytes)
    } else {
        None
    };
    let was_loaded = check_job(root, &meta)?.is_some();
    if was_loaded && old_plist.is_none() {
        return Err("RESOURCE_MONITOR_FOREIGN_PRESERVED".into());
    }
    let old_settings = Some(private_bytes(&root.join("settings.json"))?);
    let old_owner = Some(private_bytes(&root.join("owner.json"))?);
    let id = uuid::Uuid::new_v4().to_string();
    let bytes = plist(root, &meta, &config, &binary)?;
    let result = (|| -> Result<(), ControlError> {
        if was_loaded {
            control(
                root,
                &id,
                &["bootout".into(), format!("{}/{}", domain(), meta.label)],
            )?;
        }
        write_json(&root.join("settings.json"), &config).map_err(|_| ControlError::Failed)?;
        atomic(&file, &bytes).map_err(|_| ControlError::Failed)?;
        meta.binary_path = binary.to_string_lossy().into_owned();
        meta.binary_sha256 = image;
        meta.prepared = false;
        meta.plist_sha256 = Some(hash(&bytes));
        write_json(&root.join("owner.json"), &meta).map_err(|_| ControlError::Failed)?;
        if config["Enabled"] == true {
            control(
                root,
                &id,
                &[
                    "bootstrap".into(),
                    domain(),
                    file.to_string_lossy().into_owned(),
                ],
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            let _ = fs::remove_file(root.join("apply-pending.json"));
            drop(lock);
            let actual = snapshot(root)?;
            if actual["running"] != config["Enabled"] {
                return Err("RESOURCE_MONITOR_APPLY_UNCONFIRMED".into());
            }
            Ok(actual)
        }
        Err(ControlError::Unknown(mut child)) => {
            std::thread::spawn(move || {
                let _guard = lock;
                let _ = child.wait();
            });
            Err("RESOURCE_MONITOR_APPLY_UNCONFIRMED".into())
        }
        Err(ControlError::Failed) => {
            // Only this exact marked job and its private files may be restored.
            let rollback = (|| -> Result<(), ControlError> {
                if check_job(root, &meta)
                    .map_err(|_| ControlError::Failed)?
                    .is_some()
                {
                    control(
                        root,
                        &id,
                        &["bootout".into(), format!("{}/{}", domain(), meta.label)],
                    )?;
                }
                restore(&file, &old_plist).map_err(|_| ControlError::Failed)?;
                restore(&root.join("settings.json"), &old_settings)
                    .map_err(|_| ControlError::Failed)?;
                restore(&root.join("owner.json"), &old_owner).map_err(|_| ControlError::Failed)?;
                if was_loaded {
                    control(
                        root,
                        &id,
                        &[
                            "bootstrap".into(),
                            domain(),
                            file.to_string_lossy().into_owned(),
                        ],
                    )?;
                }
                Ok(())
            })();
            match rollback {
                Ok(()) => {
                    let _ = fs::remove_file(root.join("apply-pending.json"));
                    Err("RESOURCE_MONITOR_COMMAND_FAILED".into())
                }
                Err(ControlError::Unknown(mut child)) => {
                    std::thread::spawn(move || {
                        let _guard = lock;
                        let _ = child.wait();
                    });
                    Err("RESOURCE_MONITOR_ROLLBACK_UNCONFIRMED".into())
                }
                Err(ControlError::Failed) => Err("RESOURCE_MONITOR_ROLLBACK_UNCONFIRMED".into()),
            }
        }
    }
}
pub(crate) fn apply(config: Value) -> Result<Value, String> {
    apply_at(
        &root().ok_or("RESOURCE_MONITOR_STORAGE_UNAVAILABLE")?,
        config,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_rules_use_exact_uuids_and_do_not_turn_paths_into_grants() {
        let id = uuid::Uuid::new_v4().to_string();
        let mut config = json!({"SchemaVersion":3,"Enabled":false,"CommitPercent":0,"AvailableMemoryPercent":10,"SampleSeconds":120,"RepeatMinutes":30,"Disks":[{"Drive":id,"Enabled":true,"FreePercent":5}]});
        let drives = json!([{"drive":id}]);
        assert!(validate(&config, &drives).is_ok());
        config["Disks"][0]["Drive"] = json!("/Volumes/foreign");
        assert!(validate(&config, &drives).is_err());
        config["Disks"][0]["Drive"] = json!(id);
        assert!(validate(&config, &json!([])).is_err());
        config["CommitPercent"] = json!(90);
        assert!(validate(&config, &drives).is_err());
    }
    #[test]
    fn native_snapshot_is_read_only_and_a_disabled_fixture_never_installs_a_job() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("collector");
        let initial = snapshot(&root).unwrap();
        assert_eq!(initial["platform"], "MACOS");
        assert!(!root.exists());
        let config = initial["config"].clone();
        let after = apply_at(&root, config, true).unwrap();
        assert_eq!(after["installed"], true);
        assert_eq!(after["running"], false);
        let meta = owner(&root).unwrap().unwrap();
        assert!(job(&meta.label).unwrap().is_none());
        // A modified file is foreign even when its filename is ours.
        fs::write(root.join("collector.plist"), b"foreign fixture").unwrap();
        assert_eq!(
            apply_at(&root, after["config"].clone(), true).unwrap_err(),
            "RESOURCE_MONITOR_FOREIGN_PRESERVED"
        );
        assert_eq!(
            fs::read(root.join("collector.plist")).unwrap(),
            b"foreign fixture"
        );
    }
    #[test]
    fn real_owned_launchd_reconfiguration_rolls_back_and_preserves_a_second_fixture() {
        struct Fixtures(tempfile::TempDir);
        impl Drop for Fixtures {
            fn drop(&mut self) {
                for name in ["main", "protected"] {
                    let root = self.0.path().join(name);
                    if let Ok(Some(meta)) = owner(&root) {
                        if meta.is_test && check_job(&root, &meta).ok().flatten().is_some() {
                            let _ = Command::new("/bin/launchctl")
                                .args(["bootout", &format!("{}/{}", domain(), meta.label)])
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .status();
                        }
                    }
                }
            }
        }
        let fixtures = Fixtures(tempfile::tempdir().unwrap());
        let main = fixtures.0.path().join("main");
        let protected = fixtures.0.path().join("protected");
        let mut config = snapshot(&main).unwrap()["config"].clone();
        config["Enabled"] = json!(true);
        config["SampleSeconds"] = json!(60);
        assert_eq!(
            apply_at(&main, config.clone(), true).unwrap()["running"],
            true
        );
        assert_eq!(
            apply_at(&protected, config.clone(), true).unwrap()["running"],
            true
        );
        let old = private_bytes(&main.join("settings.json")).unwrap();
        let old_plist = private_bytes(&main.join("collector.plist")).unwrap();
        config["SampleSeconds"] = json!(120);
        FAIL_BOOTSTRAP.with(|flag| flag.set(true));
        assert_eq!(
            apply_at(&main, config, true).unwrap_err(),
            "RESOURCE_MONITOR_COMMAND_FAILED"
        );
        assert_eq!(private_bytes(&main.join("settings.json")).unwrap(), old);
        assert_eq!(
            private_bytes(&main.join("collector.plist")).unwrap(),
            old_plist
        );
        assert_eq!(snapshot(&main).unwrap()["running"], true);
        assert_eq!(snapshot(&protected).unwrap()["running"], true);
        for root in [&main, &protected] {
            let mut config = snapshot(root).unwrap()["config"].clone();
            config["Enabled"] = json!(false);
            assert_eq!(apply_at(root, config, true).unwrap()["running"], false);
        }
        println!("MACOS_OWNED_LAUNCHD_ROLLBACK_PASS");
    }
    #[test]
    fn uncertain_native_control_is_inspected_and_never_replayed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("collector");
        let initial = snapshot(&root).unwrap();
        let config = initial["config"].clone();
        apply_at(&root, config.clone(), true).unwrap();
        let meta = owner(&root).unwrap().unwrap();
        let native = identity(std::process::id()).unwrap();
        write_json(
            &root.join("apply-pending.json"),
            &json!({"requestId":uuid::Uuid::new_v4().to_string(),"child":native}),
        )
        .unwrap();
        assert_eq!(
            snapshot(&root).unwrap_err(),
            "RESOURCE_MONITOR_APPLY_UNCONFIRMED"
        );
        assert_eq!(
            apply_at(&root, config, true).unwrap_err(),
            "RESOURCE_MONITOR_APPLY_UNCONFIRMED"
        );
        assert!(job(&meta.label).unwrap().is_none());
        // An uncheckpointed outcome stays fenced even after a Host restart.
        write_json(
            &root.join("apply-pending.json"),
            &json!({"requestId":uuid::Uuid::new_v4().to_string(),"child":null}),
        )
        .unwrap();
        assert_eq!(
            snapshot(&root).unwrap_err(),
            "RESOURCE_MONITOR_APPLY_UNCONFIRMED"
        );
    }
}
