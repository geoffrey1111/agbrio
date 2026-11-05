//! Signature-bound staging only. Does not replace/launch an installed app, obtain
//! privileges, clear an install intent, modify accounts or change system trust.
use super::{digest, scan, Header, Inspected, Kind, Record, APP};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
// Public Darwin ACL API, Apple Libc include/sys/acl.h. This removes inherited
// filesystem ACLs only from the newly created workspace, never Keychain ACLs.
extern "C" {
    fn acl_init(count: libc::c_int) -> *mut libc::c_void;
    fn acl_free(value: *mut libc::c_void) -> libc::c_int;
    fn acl_set_fd_np(fd: libc::c_int, value: *mut libc::c_void, kind: libc::c_int) -> libc::c_int;
    fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> *mut libc::c_void;
    fn acl_get_entry(
        value: *mut libc::c_void,
        entry_id: libc::c_int,
        entry: *mut *mut libc::c_void,
    ) -> libc::c_int;
}
use std::os::fd::AsRawFd;
struct Acl(*mut libc::c_void);
impl Drop for Acl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                acl_free(self.0);
            }
        }
    }
}
fn empty_acl(file: &File) -> Result<(), String> {
    let value = Acl(unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) });
    if value.0.is_null() {
        // Darwin may represent an absent extended ACL as NULL/ENOENT.
        if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
            return Ok(());
        }
        return Err(failed());
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(value.0, 0, &mut entry) };
    let code = std::io::Error::last_os_error().raw_os_error();
    // Darwin returns 0 for an entry, -1/EINVAL for no first entry in a valid ACL.
    if result != -1 || code != Some(libc::EINVAL) {
        return Err("MAC_UPDATE_WORKSPACE_NOT_PRIVATE".into());
    }
    Ok(())
}
fn privatize_workspace(path: &Path, identity: &fs::Metadata) -> Result<(), String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| failed())?;
    if !same(identity, &file.metadata().map_err(|_| failed())?) {
        return Err(failed());
    }
    let acl = Acl(unsafe { acl_init(0) });
    if acl.0.is_null() {
        return Err(failed());
    }
    if unsafe { acl_set_fd_np(file.as_raw_fd(), acl.0, 0x100) } != 0 {
        return Err("MAC_UPDATE_WORKSPACE_NOT_PRIVATE".into());
    }
    file.sync_all().map_err(|_| failed())?;
    empty_acl(&file)
}
fn failed() -> String {
    "MAC_UPDATE_STAGING_FAILED".into()
}
fn trusted(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(failed());
    }
    for ancestor in path.ancestors() {
        let m = fs::symlink_metadata(ancestor).map_err(|_| failed())?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(failed());
        }
    }
    Ok(())
}
fn same(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino() && a.uid() == b.uid() && a.mode() == b.mode()
}
pub(crate) struct Staged {
    workspace: PathBuf,
    workspace_identity: fs::Metadata,
    app: PathBuf,
    archive_sha256: String,
    version: String,
}
impl Staged {
    pub(crate) fn app(&self) -> &Path {
        &self.app
    }
    pub(crate) fn workspace(&self) -> &Path {
        &self.workspace
    }
    pub(crate) fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    pub(crate) fn version(&self) -> &str {
        &self.version
    }
    pub(crate) fn verify(&self, inspected: &Inspected) -> Result<(), String> {
        if self.archive_sha256 != inspected.archive_sha256 || self.version != inspected.version {
            return Err(failed());
        }
        trusted(&self.workspace)?;
        if !same(
            &self.workspace_identity,
            &fs::symlink_metadata(&self.workspace).map_err(|_| failed())?,
        ) {
            return Err(failed());
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.workspace)
            .map_err(|_| failed())?;
        if !same(
            &self.workspace_identity,
            &directory.metadata().map_err(|_| failed())?,
        ) {
            return Err(failed());
        }
        empty_acl(&directory)?;
        verify_tree(&self.workspace, inspected)?;
        verify_codesign(&self.app)
    }
}

fn owned_file(path: &Path, mode: u32) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| failed())
}
fn verify_tree(workspace: &Path, inspected: &Inspected) -> Result<(), String> {
    let uid = unsafe { libc::geteuid() };
    let device = fs::symlink_metadata(workspace).map_err(|_| failed())?.dev();
    let mut seen = BTreeSet::new();
    let mut todo = vec![workspace.join(APP)];
    while let Some(path) = todo.pop() {
        if seen.len() >= super::MAX_FILES {
            return Err(failed());
        }
        let relative = path
            .strip_prefix(workspace)
            .map_err(|_| failed())?
            .to_str()
            .ok_or_else(failed)?
            .to_owned();
        let record = inspected.records.get(&relative).ok_or_else(failed)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| failed())?;
        if metadata.uid() != uid || metadata.dev() != device || !seen.insert(relative) {
            return Err(failed());
        }
        match &record.kind {
            Kind::Directory => {
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || metadata.mode() & 0o7777 != record.mode
                {
                    return Err(failed());
                }
                for entry in fs::read_dir(&path).map_err(|_| failed())? {
                    todo.push(entry.map_err(|_| failed())?.path());
                    if todo.len() + seen.len() > super::MAX_FILES {
                        return Err(failed());
                    }
                }
                if todo.len() + seen.len() > super::MAX_FILES {
                    return Err(failed());
                }
            }
            Kind::Symlink { target } => {
                if !metadata.file_type().is_symlink() || metadata.nlink() != 1 {
                    return Err(failed());
                }
                let actual = fs::read_link(&path).map_err(|_| failed())?;
                if actual.as_os_str().as_bytes() != target.as_bytes() {
                    return Err(failed());
                }
            }
            Kind::File { size, sha256 } => {
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.nlink() != 1
                    || metadata.len() != *size
                    || metadata.mode() & 0o7777 != record.mode
                {
                    return Err(failed());
                }
                let mut file = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(&path)
                    .map_err(|_| failed())?;
                if !same(&metadata, &file.metadata().map_err(|_| failed())?) {
                    return Err(failed());
                }
                use sha2::{Digest, Sha256};
                let mut hash = Sha256::new();
                let mut buffer = [0; 65536];
                let mut read = 0u64;
                loop {
                    let n = file.read(&mut buffer).map_err(|_| failed())?;
                    if n == 0 {
                        break;
                    }
                    read += n as u64;
                    if read > *size {
                        return Err(failed());
                    }
                    hash.update(&buffer[..n]);
                }
                if read != *size
                    || format!("{:x}", hash.finalize()) != *sha256
                    || !same(
                        &metadata,
                        &fs::symlink_metadata(&path).map_err(|_| failed())?,
                    )
                {
                    return Err(failed());
                }
            }
        }
    }
    if seen.len() != inspected.records.len() {
        return Err(failed());
    }
    Ok(())
}
fn verify_codesign(app: &Path) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| failed())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("MAC_UPDATE_CODESIGN_INVALID".into())
                }
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failed());
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("MAC_UPDATE_CODESIGN_UNCONFIRMED".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Caller supplies the canonical current installation's parent, not an external
/// request path. Only a fresh UUID workspace is created; installed files untouched.
pub(crate) fn prepare(
    parent: &Path,
    transaction_id: &str,
    inspected: &Inspected,
    bytes: &[u8],
) -> Result<Staged, String> {
    let uid = unsafe { libc::geteuid() };
    if uid == 0 {
        return Err("MAC_UPDATE_OWNER_CONTEXT_REQUIRED".into());
    }
    let id = uuid::Uuid::parse_str(transaction_id).map_err(|_| failed())?;
    if id.get_version_num() != 4
        || id.to_string() != transaction_id
        || digest(bytes) != inspected.archive_sha256
    {
        return Err(failed());
    }
    trusted(parent)?;
    let parent_identity = fs::symlink_metadata(parent).map_err(|_| failed())?;
    if !matches!(parent_identity.uid(), 0) && parent_identity.uid() != uid {
        return Err(failed());
    }
    if parent_identity.mode() & 0o002 != 0
        || fs::symlink_metadata(parent.join(".agbrio-update.active.json")).is_ok()
    {
        return Err("MAC_UPDATE_EXISTING_INTENT_OR_UNTRUSTED_PARENT".into());
    }
    let workspace = parent.join(format!(".agbrio-update-{transaction_id}"));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&workspace)
        .map_err(|_| failed())?;
    let workspace_identity = fs::symlink_metadata(&workspace).map_err(|_| failed())?;
    if workspace_identity.uid() != uid
        || workspace_identity.mode() & 0o7777 != 0o700
        || workspace_identity.dev() != parent_identity.dev()
    {
        return Err(failed());
    }
    privatize_workspace(&workspace, &workspace_identity)?;
    if fs::read_dir(&workspace)
        .map_err(|_| failed())?
        .next()
        .is_some()
    {
        return Err(failed());
    }
    // Failure preserves this exact partial workspace; no unbound recursive cleanup.
    for record in inspected.records.values() {
        if matches!(record.kind, Kind::Directory) {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(workspace.join(&record.path))
                .map_err(|_| failed())?;
        }
    }
    let mut open = |header: &Header| -> Result<Option<File>, String> {
        let Some(Record {
            mode,
            kind: Kind::File { size, .. },
            ..
        }) = inspected.records.get(&header.path)
        else {
            return Err(failed());
        };
        if header.kind != b'f' || header.mode != *mode || header.size != *size {
            return Err(failed());
        }
        Ok(Some(owned_file(&workspace.join(&header.path), 0o600)?))
    };
    let extracted = scan(bytes, &inspected.version, &mut open)?;
    if extracted.records != inspected.records
        || extracted.archive_sha256 != inspected.archive_sha256
    {
        return Err(failed());
    }
    for record in inspected.records.values() {
        match &record.kind {
            Kind::File { .. } => fs::set_permissions(
                workspace.join(&record.path),
                fs::Permissions::from_mode(record.mode),
            )
            .map_err(|_| failed())?,
            Kind::Symlink { target } => {
                std::os::unix::fs::symlink(target, workspace.join(&record.path))
                    .map_err(|_| failed())?
            }
            Kind::Directory => {}
        }
    }
    // Apply final directory modes from children to parents, after all writes.
    for record in inspected.records.values().rev() {
        if matches!(record.kind, Kind::Directory) {
            fs::set_permissions(
                workspace.join(&record.path),
                fs::Permissions::from_mode(record.mode),
            )
            .map_err(|_| failed())?;
        }
    }
    if !same(
        &parent_identity,
        &fs::symlink_metadata(parent).map_err(|_| failed())?,
    ) {
        return Err(failed());
    }
    let staged = Staged {
        app: workspace.join(APP),
        workspace,
        workspace_identity,
        version: inspected.version.clone(),
        archive_sha256: inspected.archive_sha256.clone(),
    };
    staged.verify(inspected)?;
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[track_caller]
    fn tool(path: &str, args: &[&std::ffi::OsStr]) {
        let status = Command::new(path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "fixture tool {path} failed: {status}");
    }
    #[test]
    #[ignore = "GitHub temporary Mac fixture; compiles benign no-op, ad-hoc signing only, no install/accounts"]
    fn isolated_native_archive_staging_and_tamper_rejection() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        assert_eq!(std::env::var("RUNNER_OS").as_deref(), Ok("macOS"));
        assert_ne!(unsafe { libc::geteuid() }, 0);
        let root = PathBuf::from(std::env::var_os("RUNNER_TEMP").unwrap())
            .canonicalize()
            .unwrap();
        let temp = tempfile::tempdir_in(&root).unwrap();
        let base = temp.path().canonicalize().unwrap();
        assert!(base.starts_with(root));
        let source = base.join(APP);
        let macos = source.join("Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        let c = base.join("benign.c");
        fs::write(&c, b"int main(void) { return 0; }\n").unwrap();
        let executable = macos.join("ai-work-router");
        tool(
            "/usr/bin/clang",
            &[
                c.as_os_str(),
                std::ffi::OsStr::new("-arch"),
                std::ffi::OsStr::new("arm64"),
                std::ffi::OsStr::new("-mmacosx-version-min=13.5"),
                std::ffi::OsStr::new("-o"),
                executable.as_os_str(),
            ],
        );
        let node = macos.join("browser-executor-node");
        fs::copy(&executable, &node).unwrap();
        let mut info = plist::Dictionary::new();
        for (k, v) in [
            ("CFBundleIdentifier", "com.geoffrey.aiworkrouter"),
            ("CFBundleExecutable", "ai-work-router"),
            ("CFBundleName", "Agbrio"),
            ("CFBundlePackageType", "APPL"),
            ("CFBundleVersion", "0.1.1"),
            ("CFBundleShortVersionString", "0.1.1"),
            ("LSMinimumSystemVersion", "13.5"),
        ] {
            info.insert(k.into(), plist::Value::String(v.into()));
        }
        plist::Value::Dictionary(info)
            .to_file_xml(source.join("Contents/Info.plist"))
            .unwrap();
        let data = source.join("Contents/Resources/data");
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("fixture.txt"), b"owned benign data").unwrap();
        std::os::unix::fs::symlink("data", source.join("Contents/Resources/current")).unwrap();
        for path in [&node, &executable, &source] {
            tool(
                "/usr/bin/codesign",
                &[
                    std::ffi::OsStr::new("--force"),
                    std::ffi::OsStr::new("--sign"),
                    std::ffi::OsStr::new("-"),
                    path.as_os_str(),
                ],
            );
        }
        verify_codesign(&source).unwrap();
        let archive = base.join("Agbrio_0.1.1_aarch64.app.tar.gz");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("scripts/macos-update-package.py");
        tool(
            "/usr/bin/python3",
            &[
                script.as_os_str(),
                std::ffi::OsStr::new("pack"),
                std::ffi::OsStr::new("--app"),
                source.as_os_str(),
                std::ffi::OsStr::new("--archive"),
                archive.as_os_str(),
                std::ffi::OsStr::new("--manifest"),
                base.join("fixture-manifest.json").as_os_str(),
                std::ffi::OsStr::new("--version"),
                std::ffi::OsStr::new("0.1.1"),
                std::ffi::OsStr::new("--source"),
                std::ffi::OsStr::new("0000000000000000000000000000000000000000"),
            ],
        );
        let bytes = fs::read(&archive).unwrap();
        // Test-only lower layer: no publisher key in CI. The production constructor
        // above always verifies the fixed publisher signature before issuing this type.
        let inspected = scan(&bytes, "0.1.1", |_| Ok(None)).unwrap();
        let parent = base.join("installation");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        // Only this disposable parent gets an inherited read/search ACL. The
        // extractor must remove it from the new workspace, preserving the parent.
        tool(
            "/bin/chmod",
            &[
                std::ffi::OsStr::new("+a"),
                std::ffi::OsStr::new("everyone allow list,search,file_inherit,directory_inherit"),
                parent.as_os_str(),
            ],
        );
        let parent_fd = File::open(&parent).unwrap();
        assert!(empty_acl(&parent_fd).is_err());
        fs::write(parent.join("preserved-owner-file"), b"preserve").unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let staged = prepare(&parent, &id, &inspected, &bytes).unwrap();
        assert!(empty_acl(&parent_fd).is_err());
        assert_eq!(staged.version(), "0.1.1");
        assert_eq!(staged.archive_sha256(), digest(&bytes));
        assert_eq!(
            fs::read(parent.join("preserved-owner-file")).unwrap(),
            b"preserve"
        );
        assert!(
            fs::symlink_metadata(staged.app().join("Contents/Resources/current"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(prepare(&parent, &id, &inspected, &bytes).is_err());
        let mut changed = bytes.clone();
        changed.push(1);
        let next = uuid::Uuid::new_v4().to_string();
        assert!(prepare(&parent, &next, &inspected, &changed).is_err());
        assert!(!parent.join(format!(".agbrio-update-{next}")).exists());
        let alias = base.join("alias");
        std::os::unix::fs::symlink(&parent, &alias).unwrap();
        assert!(prepare(
            &alias,
            &uuid::Uuid::new_v4().to_string(),
            &inspected,
            &bytes
        )
        .is_err());
        let mut altered = OpenOptions::new()
            .append(true)
            .open(staged.app().join("Contents/Resources/data/fixture.txt"))
            .unwrap();
        altered.write_all(b"changed").unwrap();
        drop(altered);
        assert!(staged.verify(&inspected).is_err());
        assert!(staged.workspace().exists());
        println!("\nMACOS_ARCHIVE_STAGING_NATIVE_PASS");
    }
}
