//! Explicit grant to one already-running current-user Cloudflared launch agent.
//! Only process metadata and plist filesystem identity are inspected. No plist
//! contents, argv, environment, credentials or tunnel routes are read/changed.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const LABELS: [&str; 2] = ["com.cloudflare.cloudflared", "homebrew.mxcl.cloudflared"];
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fingerprint {
    device: u64,
    inode: u64,
    bytes: u64,
    modified: i64,
    modified_ns: i64,
    changed: i64,
    changed_ns: i64,
    uid: u32,
    resolved: String,
    link_inode: u64,
    link_changed: i64,
    link_changed_ns: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Grant {
    schema: String,
    uid: u32,
    label: String,
    plist: PathBuf,
    config: Fingerprint,
    binary: PathBuf,
    binary_sha256: String,
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
fn directory() -> Result<PathBuf, String> {
    Ok(router_core::platform::local_data_base()?.join("AIWorkRouter/web-recovery/macos"))
}
fn image_hash(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|_| "WEB_RECOVERY_IMAGE_UNAVAILABLE")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn fingerprint(path: &Path) -> Result<Fingerprint, String> {
    let link = fs::symlink_metadata(path).map_err(|_| "WEB_RECOVERY_CONFIG_UNAVAILABLE")?;
    let resolved = fs::canonicalize(path).map_err(|_| "WEB_RECOVERY_CONFIG_UNAVAILABLE")?;
    let home = router_core::platform::user_home()?;
    if ![
        home.as_path(),
        Path::new("/opt/homebrew"),
        Path::new("/usr/local"),
    ]
    .iter()
    .any(|root| resolved.starts_with(root))
    {
        return Err("WEB_RECOVERY_CONFIG_UNTRUSTED".into());
    }
    let value = fs::metadata(&resolved).map_err(|_| "WEB_RECOVERY_CONFIG_UNAVAILABLE")?;
    if !value.is_file() {
        return Err("WEB_RECOVERY_CONFIG_UNTRUSTED".into());
    }
    Ok(Fingerprint {
        device: value.dev(),
        inode: value.ino(),
        bytes: value.len(),
        modified: value.mtime(),
        modified_ns: value.mtime_nsec(),
        changed: value.ctime(),
        changed_ns: value.ctime_nsec(),
        uid: value.uid(),
        resolved: resolved.to_string_lossy().into_owned(),
        link_inode: link.ino(),
        link_changed: link.ctime(),
        link_changed_ns: link.ctime_nsec(),
    })
}
fn jobs() -> Result<Vec<(String, u32)>, String> {
    let output = Command::new("/bin/launchctl")
        .arg("list")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "WEB_RECOVERY_SERVICE_UNAVAILABLE")?;
    if !output.status.success() || output.stdout.len() > 1024 * 1024 {
        return Err("WEB_RECOVERY_SERVICE_UNAVAILABLE".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            (parts.len() == 3)
                .then(|| {
                    parts[0]
                        .parse::<u32>()
                        .ok()
                        .map(|pid| (parts[2].to_owned(), pid))
                })
                .flatten()
        })
        .collect())
}
fn native(label: &str) -> Result<Value, String> {
    let pid = jobs()?
        .into_iter()
        .find(|(name, _)| name == label)
        .map(|(_, pid)| pid)
        .ok_or("WEB_RECOVERY_RUNNING_USER_SERVICE_REQUIRED")?;
    let value = super::macos_resources::identity(pid)?;
    if value["ownerUid"] != uid() {
        return Err("WEB_RECOVERY_PRIVILEGED_SERVICE_REQUIRES_DESIGN".into());
    }
    Ok(value)
}
fn private_json(path: &Path) -> Result<Value, String> {
    let info = fs::symlink_metadata(path).map_err(|_| "WEB_RECOVERY_SETUP_REQUIRED")?;
    if !info.is_file()
        || info.file_type().is_symlink()
        || info.uid() != uid()
        || info.mode() & 0o077 != 0
        || info.nlink() != 1
        || info.len() > 16384
    {
        return Err("WEB_RECOVERY_GRANT_UNTRUSTED".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| "WEB_RECOVERY_SETUP_REQUIRED")?)
        .map_err(|_| "WEB_RECOVERY_GRANT_UNTRUSTED".into())
}
fn root_safe(root: &Path, create: bool) -> Result<(), String> {
    for p in root.ancestors() {
        if fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("WEB_RECOVERY_GRANT_UNTRUSTED".into());
        }
    }
    if create {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|_| "WEB_RECOVERY_STORAGE_UNAVAILABLE")?;
    }
    let info = fs::metadata(root).map_err(|_| "WEB_RECOVERY_SETUP_REQUIRED")?;
    if !info.is_dir() || info.uid() != uid() || info.mode() & 0o777 != 0o700 {
        return Err("WEB_RECOVERY_GRANT_UNTRUSTED".into());
    }
    Ok(())
}
fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("WEB_RECOVERY_STORAGE_UNAVAILABLE")?;
    root_safe(parent, true)?;
    if path.exists() {
        let _ = private_json(path)?;
    }
    let temp = parent.join(format!(".web-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|_| "WEB_RECOVERY_STORAGE_UNAVAILABLE")?;
        file.write_all(&serde_json::to_vec(value).map_err(|_| "WEB_RECOVERY_STORAGE_UNAVAILABLE")?)
            .and_then(|_| file.sync_all())
            .map_err(|_| "WEB_RECOVERY_STORAGE_UNAVAILABLE")?;
        fs::rename(&temp, path).map_err(|_| "WEB_RECOVERY_STORAGE_UNAVAILABLE".into())
    })();
    let _ = fs::remove_file(temp);
    result
}
fn lock(root: &Path) -> Result<std::fs::File, String> {
    root_safe(root, true)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(0x100)
        .open(root.join("operation.lock"))
        .map_err(|_| "WEB_RECOVERY_SETUP_BUSY")?;
    let info = file.metadata().map_err(|_| "WEB_RECOVERY_SETUP_BUSY")?;
    if !info.is_file() || info.uid() != uid() || info.mode() & 0o777 != 0o600 || info.nlink() != 1 {
        return Err("WEB_RECOVERY_GRANT_UNTRUSTED".into());
    }
    file.try_lock().map_err(|_| "WEB_RECOVERY_SETUP_BUSY")?;
    Ok(file)
}
fn pending_settled(root: &Path) -> Result<bool, String> {
    let file = root.join("pending.json");
    if !file.exists() {
        return Ok(true);
    }
    let value = private_json(&file)?;
    if value["terminal"] == true {
        return Ok(true);
    }
    let Some(pid) = value["child"]["pid"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
    else {
        return Ok(false);
    };
    let actual = super::macos_resources::identity(pid)?;
    Ok(actual["state"] == "EXITED"
        || actual["created"] != value["child"]["created"]
        || actual["ownerUid"] != value["child"]["ownerUid"])
}
fn grant_at(root: &Path, label: &str, plist: &Path, expected_name: &str) -> Result<(), String> {
    let _lock = lock(root)?;
    if !pending_settled(root)? {
        return Err("WEB_RECOVERY_UNCONFIRMED".into());
    }
    let value = native(label)?;
    let binary = PathBuf::from(
        value["image"]
            .as_str()
            .ok_or("WEB_RECOVERY_IMAGE_UNAVAILABLE")?,
    );
    if binary.file_name().and_then(|v| v.to_str()) != Some(expected_name) {
        return Err("WEB_RECOVERY_SERVICE_UNTRUSTED".into());
    }
    let grant = Grant {
        schema: "AGBRIO_MAC_WEB_RECOVERY_V1".into(),
        uid: uid(),
        label: label.into(),
        plist: plist.into(),
        config: fingerprint(plist)?,
        binary_sha256: image_hash(&binary)?,
        binary,
    };
    write(&root.join("grant.json"), &grant)?;
    let _ = fs::remove_file(root.join("pending.json"));
    Ok(())
}
pub(crate) fn setup() -> Result<(), String> {
    let home = router_core::platform::user_home()?;
    let found: Vec<_> = jobs()?
        .into_iter()
        .filter(|(label, _)| LABELS.contains(&label.as_str()))
        .collect();
    if found.len() != 1 {
        return Err("WEB_RECOVERY_RUNNING_USER_SERVICE_REQUIRED".into());
    }
    let label = &found[0].0;
    grant_at(
        &directory()?,
        label,
        &home
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist")),
        "cloudflared",
    )
}
fn inspect(root: &Path) -> Result<(Grant, Value), String> {
    root_safe(root, false)?;
    if root.join("pending.json").exists() {
        return Err("WEB_RECOVERY_UNCONFIRMED".into());
    }
    let grant: Grant = serde_json::from_value(private_json(&root.join("grant.json"))?)
        .map_err(|_| "WEB_RECOVERY_GRANT_UNTRUSTED")?;
    let label_allowed = LABELS.contains(&grant.label.as_str());
    #[cfg(test)]
    let label_allowed = label_allowed || grant.label.starts_with("com.geoffrey.agbrio.webfixture.");
    let image_allowed = grant.binary.file_name().and_then(|v| v.to_str()) == Some("cloudflared");
    #[cfg(test)]
    let image_allowed = image_allowed || grant.binary == Path::new("/bin/sleep");
    if grant.schema != "AGBRIO_MAC_WEB_RECOVERY_V1"
        || grant.uid != uid()
        || !label_allowed
        || !image_allowed
        || fingerprint(&grant.plist)? != grant.config
        || image_hash(&grant.binary)? != grant.binary_sha256
    {
        return Err("WEB_RECOVERY_SETUP_REQUIRED".into());
    }
    let value = native(&grant.label)?;
    if value["image"].as_str() != grant.binary.to_str() {
        return Err("WEB_RECOVERY_SERVICE_UNTRUSTED".into());
    }
    Ok((grant, value))
}
pub(crate) fn can_recover() -> bool {
    directory().ok().is_some_and(|root| inspect(&root).is_ok())
}
fn recover_at(root: &Path, stop: &AtomicBool) -> Result<(), String> {
    let guard = lock(root)?;
    let (grant, before) = inspect(root)?;
    if stop.load(Ordering::Acquire) {
        return Err("WEB_RECOVERY_CANCELLED".into());
    }
    let check = native(&grant.label)?;
    if check["pid"] != before["pid"]
        || check["created"] != before["created"]
        || fingerprint(&grant.plist)? != grant.config
    {
        return Err("WEB_RECOVERY_SERVICE_CHANGED".into());
    }
    write(
        &root.join("pending.json"),
        &json!({"requestId":uuid::Uuid::new_v4().to_string(),"child":null}),
    )?;
    let mut child = Command::new("/bin/launchctl")
        .args(["kickstart", "-k", &format!("gui/{}/{}", uid(), grant.label)])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "WEB_RECOVERY_UNCONFIRMED")?;
    if let Ok(value) = super::macos_resources::identity(child.id()) {
        let _ = write(&root.join("pending.json"), &json!({"child":value}));
    }
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => {
                std::thread::spawn(move || {
                    let _guard = guard;
                    let _ = child.wait();
                });
                return Err("WEB_RECOVERY_UNCONFIRMED".into());
            }
        }
    };
    write(&root.join("pending.json"), &json!({"terminal":true}))?;
    if !status.success() {
        return Err("WEB_RECOVERY_COMMAND_FAILED".into());
    }
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if stop.load(Ordering::Acquire) {
            return Err("WEB_RECOVERY_UNCONFIRMED".into());
        }
        if let Ok(after) = native(&grant.label) {
            if after["image"].as_str() == grant.binary.to_str()
                && after["created"] != before["created"]
            {
                let _ = fs::remove_file(root.join("pending.json"));
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("WEB_RECOVERY_UNCONFIRMED".into())
}
pub(crate) fn recover(stop: &AtomicBool) -> Result<(), String> {
    recover_at(&directory()?, stop)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plist_identity_is_metadata_only_and_changes_require_new_owner_setup() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("service.plist");
        fs::write(&file, b"fixture secret must not be parsed").unwrap();
        let first = fingerprint(&file).unwrap();
        fs::write(&file, b"changed fixture secret").unwrap();
        assert!(fingerprint(&file).unwrap() != first);
    }
    #[test]
    fn exact_running_user_service_recovers_without_touching_another_fixture_or_reading_config_values(
    ) {
        struct Services(Vec<String>);
        impl Drop for Services {
            fn drop(&mut self) {
                for label in &self.0 {
                    let _ = Command::new("/bin/launchctl")
                        .args(["bootout", &format!("gui/{}/{label}", uid())])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let mut services = Services(Vec::new());
        let mut paths = Vec::new();
        for _ in 0..2 {
            let label = format!(
                "com.geoffrey.agbrio.webfixture.{}",
                uuid::Uuid::new_v4().simple()
            );
            let file = temp.path().join(format!("{label}.plist"));
            let text=format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>/bin/sleep</string><string>60</string></array><key>RunAtLoad</key><true/><key>EnvironmentVariables</key><dict><key>AGBRIO_FIXTURE_SECRET</key><string>not-for-projection</string></dict></dict></plist>");
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&file)
                .unwrap();
            output.write_all(text.as_bytes()).unwrap();
            assert!(Command::new("/bin/launchctl")
                .args([
                    "bootstrap",
                    &format!("gui/{}", uid()),
                    file.to_str().unwrap()
                ])
                .status()
                .unwrap()
                .success());
            services.0.push(label);
            paths.push(file);
        }
        let until = Instant::now() + Duration::from_secs(5);
        let protected = loop {
            if let Ok(value) = native(&services.0[1]) {
                break value;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(50));
        };
        let root = temp.path().join("grant");
        grant_at(&root, &services.0[0], &paths[0], "sleep").unwrap();
        let (_, before) = inspect(&root).unwrap();
        recover_at(&root, &AtomicBool::new(false)).unwrap();
        let (_, after) = inspect(&root).unwrap();
        assert_ne!(before["created"], after["created"]);
        let untouched = native(&services.0[1]).unwrap();
        assert_eq!(untouched["pid"], protected["pid"]);
        assert_eq!(untouched["created"], protected["created"]);
        assert!(!fs::read_to_string(root.join("grant.json"))
            .unwrap()
            .contains("not-for-projection"));
        fs::write(&paths[0], b"changed configuration fixture").unwrap();
        assert!(inspect(&root).is_err());
        println!("MACOS_USER_WEB_RECOVERY_FIXTURE_PASS");
    }
}
