//! Current-user Darwin driver. Only two fixed bundle names in pinned directories
//! can be exchanged. No shell, root relay, recursive deletion or process control.
use super::{Binding, Disk, Identity, Layout, Phase};
use std::{
    ffi::{CStr, CString},
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::{ffi::OsStrExt, fs::FileExt},
    path::{Component, Path, PathBuf},
};

const APP: &[u8] = b"Agbrio.app\0";
const LEASE: &[u8] = b".agbrio-update.lock\0";
const JOURNAL: &[u8] = b"journal.jsonl\0";
const ACTIVE: &[u8] = b".agbrio-update.active.json\0";
const LEASE_MARKER: &[u8] = b"agbrio-update-lock-v1\n";
fn name(value: &[u8]) -> &CStr {
    CStr::from_bytes_with_nul(value).expect("fixed native name")
}
fn fail() -> String {
    "MAC_UPDATE_NATIVE_UNAVAILABLE".into()
}
fn file(fd: libc::c_int) -> Result<File, String> {
    if fd < 0 {
        Err(fail())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn stat_fd(file: &File) -> Result<libc::stat, String> {
    let mut value = MaybeUninit::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
        return Err(fail());
    }
    Ok(unsafe { value.assume_init() })
}
fn stat_at(parent: &File, path: &CStr) -> Result<libc::stat, String> {
    let mut value = MaybeUninit::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            path.as_ptr(),
            value.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(fail());
    }
    Ok(unsafe { value.assume_init() })
}
fn directory(value: libc::stat) -> Result<Identity, String> {
    if value.st_mode & libc::S_IFMT != libc::S_IFDIR {
        return Err("MAC_UPDATE_LINK_OR_TYPE_REJECTED".into());
    }
    Ok(Identity {
        device: value.st_dev as u64,
        inode: value.st_ino as u64,
        uid: value.st_uid,
        permissions: value.st_mode as u32 & 0o7777,
    })
}
fn same_file(a: &libc::stat, b: &libc::stat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino && a.st_uid == b.st_uid && a.st_mode == b.st_mode
}
fn open_dir(parent: &File, path: &CStr) -> Result<File, String> {
    file(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })
}
/// Resolve components from a pinned / descriptor; no ancestor symlink is followed.
fn open_absolute(path: &Path) -> Result<File, String> {
    if !path.is_absolute() {
        return Err("MAC_UPDATE_PATH_REJECTED".into());
    }
    let mut current = file(unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(part) => {
                let part = CString::new(part.as_bytes()).map_err(|_| "MAC_UPDATE_PATH_REJECTED")?;
                current = open_dir(&current, &part)?;
            }
            _ => return Err("MAC_UPDATE_PATH_REJECTED".into()),
        }
    }
    Ok(current)
}
fn sync_dir(directory: &File) -> Result<(), String> {
    directory.sync_all().map_err(|_| fail())
}
fn regular_owned(value: &libc::stat, uid: u32) -> bool {
    value.st_mode & libc::S_IFMT == libc::S_IFREG
        && value.st_uid == uid
        && value.st_nlink == 1
        && value.st_mode as u32 & 0o7777 == 0o600
}

fn lease(parent: &File, uid: u32) -> Result<File, String> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name(LEASE).as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_int,
        )
    };
    let mut lease = if fd >= 0 {
        let mut lease = file(fd)?;
        lease
            .write_all(LEASE_MARKER)
            .and_then(|_| lease.sync_all())
            .map_err(|_| fail())?;
        sync_dir(parent)?;
        lease
    } else {
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            return Err(fail());
        }
        file(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name(LEASE).as_ptr(),
                libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?
    };
    let value = stat_fd(&lease)?;
    if !regular_owned(&value, uid) || value.st_size != LEASE_MARKER.len() as i64 {
        return Err("MAC_UPDATE_FOREIGN_LEASE_PRESERVED".into());
    }
    let mut marker = vec![0; LEASE_MARKER.len()];
    lease
        .seek(SeekFrom::Start(0))
        .and_then(|_| lease.read_exact(&mut marker))
        .map_err(|_| fail())?;
    if marker != LEASE_MARKER {
        return Err("MAC_UPDATE_FOREIGN_LEASE_PRESERVED".into());
    }
    if unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("MAC_UPDATE_INSTALL_BUSY".into());
    }
    if !same_file(&stat_fd(&lease)?, &stat_at(parent, name(LEASE))?) {
        return Err("MAC_UPDATE_LEASE_CHANGED".into());
    }
    Ok(lease)
}

pub(crate) struct NativeDisk {
    parent_path: PathBuf,
    parent: File,
    workspace: File,
    workspace_name: CString,
    lease: File,
    journal: Option<File>,
    active: Option<File>,
    active_size: i64,
    uid: u32,
    bound: Binding,
    last_phase: Option<Phase>,
    swap_attempts: u8,
    last_swap_ok: bool,
    journal_size: i64,
}
fn layout(parent: &File, workspace: &File, workspace_name: &CStr) -> Result<Layout, String> {
    let parent_identity = directory(stat_fd(parent)?)?;
    let workspace_identity = directory(stat_fd(workspace)?)?;
    if directory(stat_at(parent, workspace_name)?)? != workspace_identity {
        return Err("MAC_UPDATE_WORKSPACE_CHANGED".into());
    }
    Ok(Layout {
        parent: parent_identity,
        workspace: workspace_identity,
        current: directory(stat_at(parent, name(APP))?)?,
        candidate: directory(stat_at(workspace, name(APP))?)?,
    })
}

impl NativeDisk {
    /// Only current-user writable installs in this first leaf. Root-owned bundles
    /// require the separately reviewed authorization integration; no fallback here.
    pub(crate) fn attach(
        parent_path: &Path,
        transaction_id: String,
        from_version: String,
        to_version: String,
        archive_sha256: String,
    ) -> Result<(Binding, Self), String> {
        let uid = unsafe { libc::geteuid() };
        if uid == 0 {
            return Err("MAC_UPDATE_OWNER_CONTEXT_REQUIRED".into());
        }
        let id =
            uuid::Uuid::parse_str(&transaction_id).map_err(|_| "MAC_UPDATE_INVALID_BINDING")?;
        if id.get_version_num() != 4 || id.to_string() != transaction_id {
            return Err("MAC_UPDATE_INVALID_BINDING".into());
        }
        let parent = open_absolute(parent_path)?;
        let workspace_name =
            CString::new(format!(".agbrio-update-{transaction_id}")).map_err(|_| fail())?;
        let workspace = open_dir(&parent, &workspace_name)?;
        let original = layout(&parent, &workspace, &workspace_name)?;
        let binding = Binding::new(
            transaction_id,
            from_version,
            to_version,
            archive_sha256,
            original,
            uid,
        )?;
        let lease = lease(&parent, uid)?;
        let mut disk = Self {
            parent_path: parent_path.into(),
            parent,
            workspace,
            workspace_name,
            lease,
            journal: None,
            active: None,
            active_size: 0,
            uid,
            bound: binding.clone(),
            last_phase: None,
            swap_attempts: 0,
            last_swap_ok: false,
            journal_size: 0,
        };
        if disk.snapshot()? != binding.original {
            return Err("MAC_UPDATE_LAYOUT_CHANGED".into());
        }
        Ok((binding, disk))
    }
    fn journal(&mut self, phase: Phase) -> Result<&mut File, String> {
        if phase == Phase::Intent {
            if self.journal.is_some() {
                return Err("MAC_UPDATE_EXISTING_INTENT".into());
            }
            self.journal = Some(file(unsafe {
                libc::openat(
                    self.workspace.as_raw_fd(),
                    name(JOURNAL).as_ptr(),
                    libc::O_WRONLY
                        | libc::O_APPEND
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600 as libc::c_int,
                )
            })?);
        }
        let journal = self.journal.as_mut().ok_or("MAC_UPDATE_INTENT_REQUIRED")?;
        let metadata = stat_fd(journal)?;
        if !regular_owned(&metadata, self.uid)
            || metadata.st_size > 32768
            || !same_file(&metadata, &stat_at(&self.workspace, name(JOURNAL))?)
        {
            return Err("MAC_UPDATE_JOURNAL_CHANGED".into());
        }
        Ok(journal)
    }
}
impl Disk for NativeDisk {
    fn snapshot(&mut self) -> Result<Layout, String> {
        let reopened = open_absolute(&self.parent_path)?;
        if directory(stat_fd(&reopened)?)? != directory(stat_fd(&self.parent)?)? {
            return Err("MAC_UPDATE_PARENT_CHANGED".into());
        }
        let lease_meta = stat_fd(&self.lease)?;
        if !regular_owned(&lease_meta, self.uid)
            || lease_meta.st_size != LEASE_MARKER.len() as i64
            || !same_file(&lease_meta, &stat_at(&self.parent, name(LEASE))?)
        {
            return Err("MAC_UPDATE_LEASE_CHANGED".into());
        }
        let mut marker = vec![0; LEASE_MARKER.len()];
        self.lease
            .read_exact_at(&mut marker, 0)
            .map_err(|_| fail())?;
        if marker != LEASE_MARKER {
            return Err("MAC_UPDATE_LEASE_CHANGED".into());
        }
        if let Some(journal) = &self.journal {
            let metadata = stat_fd(journal)?;
            if !regular_owned(&metadata, self.uid)
                || metadata.st_size != self.journal_size
                || !same_file(&metadata, &stat_at(&self.workspace, name(JOURNAL))?)
            {
                return Err("MAC_UPDATE_JOURNAL_CHANGED".into());
            }
        }
        if let Some(active) = &self.active {
            let metadata = stat_fd(active)?;
            if !regular_owned(&metadata, self.uid)
                || metadata.st_size != self.active_size
                || !same_file(&metadata, &stat_at(&self.parent, name(ACTIVE))?)
            {
                return Err("MAC_UPDATE_ACTIVE_FENCE_CHANGED".into());
            }
        }
        layout(&self.parent, &self.workspace, &self.workspace_name)
    }
    fn record(&mut self, binding: &Binding, phase: Phase) -> Result<(), String> {
        if binding != &self.bound {
            return Err("MAC_UPDATE_BINDING_CHANGED".into());
        }
        let allowed = matches!(
            (self.last_phase, phase, self.swap_attempts),
            (None, Phase::Intent, 0)
                | (Some(Phase::Intent), Phase::Swapped, 1)
                | (Some(Phase::Swapped), Phase::Verified, 1)
                | (Some(Phase::Swapped), Phase::RollbackIntent, 1)
                | (Some(Phase::RollbackIntent), Phase::RolledBack, 2)
        );
        if !allowed || (phase != Phase::Intent && !self.last_swap_ok) {
            return Err("MAC_UPDATE_PHASE_REJECTED".into());
        }
        // Recheck the exact bound bundle identities in this driver too, after
        // the caller's check and before writing any authoritative phase receipt.
        let expected = if matches!(phase, Phase::Intent | Phase::RolledBack) {
            binding.original.clone()
        } else {
            binding.swapped()
        };
        if self.snapshot()? != expected {
            return Err("MAC_UPDATE_LAYOUT_CHANGED".into());
        }
        let mut bytes = serde_json::to_vec(
            &serde_json::json!({"schemaVersion":1,"binding":binding,"phase":phase}),
        )
        .map_err(|_| fail())?;
        if bytes.len() > 8192 {
            return Err("MAC_UPDATE_JOURNAL_LIMIT".into());
        }
        bytes.push(b'\n');
        if phase == Phase::Intent {
            // Immutable application-wide receipt. It survives Drop/process exit,
            // even after a rollback, until a later explicit reconciliation gate.
            // A fresh UUID must never bypass an unsettled previous install.
            if self.active.is_some() {
                return Err("MAC_UPDATE_EXISTING_INTENT".into());
            }
            self.active = Some(file(unsafe {
                libc::openat(
                    self.parent.as_raw_fd(),
                    name(ACTIVE).as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600 as libc::c_int,
                )
            })?);
            let active = self.active.as_mut().expect("new active receipt");
            active
                .write_all(&bytes)
                .and_then(|_| active.sync_all())
                .map_err(|_| fail())?;
            sync_dir(&self.parent)?;
            self.active_size = bytes.len() as i64;
        }
        let journal = self.journal(phase)?;
        journal
            .write_all(&bytes)
            .and_then(|_| journal.sync_all())
            .map_err(|_| fail())?;
        sync_dir(&self.workspace)?;
        self.journal_size += bytes.len() as i64;
        self.last_phase = Some(phase);
        Ok(())
    }
    fn swap(&mut self) -> Result<(), String> {
        if !matches!(
            (self.last_phase, self.swap_attempts),
            (Some(Phase::Intent), 0) | (Some(Phase::RollbackIntent), 1)
        ) {
            return Err("MAC_UPDATE_SWAP_NOT_ADMITTED".into());
        }
        // Freeze this attempt even if a preflight/read/flush reports uncertainty.
        self.swap_attempts += 1;
        self.last_swap_ok = false;
        let expected = if self.last_phase == Some(Phase::Intent) {
            self.bound.original.clone()
        } else {
            self.bound.swapped()
        };
        if self.snapshot()? != expected {
            return Err("MAC_UPDATE_LAYOUT_CHANGED".into());
        }
        // Darwin's RENAME_SWAP exchanges both existing directory entries atomically.
        // Both dirfds were bound on the same device before obtaining the lease.
        let result = unsafe {
            libc::renameatx_np(
                self.parent.as_raw_fd(),
                name(APP).as_ptr(),
                self.workspace.as_raw_fd(),
                name(APP).as_ptr(),
                libc::RENAME_SWAP,
            )
        };
        if result != 0 {
            return Err(fail());
        }
        // Flush errors are uncertain outcomes, never permission to repeat the swap.
        sync_dir(&self.parent)?;
        sync_dir(&self.workspace)?;
        self.last_swap_ok = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::macos_update_transaction::{
        apply, inspect, Inspection, Outcome, Verification,
    };
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
    };
    fn fixture(root: &Path, label: &str) -> (PathBuf, String, PathBuf) {
        let parent = root.join(label);
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let workspace = parent.join(format!(".agbrio-update-{id}"));
        fs::create_dir(&workspace).unwrap();
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700)).unwrap();
        for (app, value) in [
            (parent.join("Agbrio.app"), b"old"),
            (workspace.join("Agbrio.app"), b"new"),
        ] {
            fs::create_dir(&app).unwrap();
            fs::set_permissions(&app, fs::Permissions::from_mode(0o755)).unwrap();
            fs::write(app.join("fixture-identity"), value).unwrap();
        }
        (parent, id, workspace)
    }
    fn attach(parent: &Path, id: String) -> Result<(Binding, NativeDisk), String> {
        NativeDisk::attach(parent, id, "0.1.0".into(), "0.1.1".into(), "a".repeat(64))
    }
    #[test]
    #[ignore = "GitHub ephemeral Mac only; own temporary trees, no app/process/account/authorization"]
    fn isolated_same_volume_exchange_and_rollback() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        assert_eq!(std::env::var("RUNNER_OS").as_deref(), Ok("macOS"));
        assert_ne!(unsafe { libc::geteuid() }, 0);
        let scratch = PathBuf::from(std::env::var_os("RUNNER_TEMP").expect("owned runner temp"))
            .canonicalize()
            .unwrap();
        let root = tempfile::tempdir_in(&scratch).unwrap();
        let root_path = root.path().canonicalize().unwrap();
        assert!(root_path.starts_with(&scratch));
        for (label, verify, expected) in [
            (
                "success",
                Verification::Verified,
                Outcome::AwaitingRestartWithBackup,
            ),
            ("reject", Verification::Rejected, Outcome::RolledBack),
            (
                "unknown",
                Verification::Unavailable,
                Outcome::NeedsInspection,
            ),
        ] {
            let (parent, id, workspace) = fixture(&root_path, label);
            let (binding, mut disk) = attach(&parent, id.clone()).unwrap();
            assert_eq!(apply(&binding, &mut disk, || verify), expected);
            let reverted = expected == Outcome::RolledBack;
            assert_eq!(
                fs::read(parent.join("Agbrio.app/fixture-identity")).unwrap(),
                if reverted { b"old" } else { b"new" }
            );
            assert_eq!(
                fs::read(workspace.join("Agbrio.app/fixture-identity")).unwrap(),
                if reverted { b"new" } else { b"old" }
            );
            assert_eq!(
                inspect(&binding, &mut disk),
                if reverted {
                    Inspection::Original
                } else {
                    Inspection::SwappedWithBackup
                }
            );
            drop(disk);
            // A new process cannot replay even a completed or partially observed intent.
            let (next, mut disk) = attach(&parent, id).unwrap();
            assert_eq!(
                apply(&next, &mut disk, || panic!("existing journal")),
                Outcome::NeedsInspection
            );
        }
        let (parent, id, _) = fixture(&root_path, "lease");
        let (_, mut disk) = attach(&parent, id.clone()).unwrap();
        assert_eq!(disk.swap().unwrap_err(), "MAC_UPDATE_SWAP_NOT_ADMITTED");
        assert!(matches!(attach(&parent,id),Err(e) if e=="MAC_UPDATE_INSTALL_BUSY"));
        drop(disk);
        let (parent, id, workspace) = fixture(&root_path, "global-fence");
        let (binding, mut disk) = attach(&parent, id).unwrap();
        assert_eq!(
            apply(&binding, &mut disk, || Verification::Unavailable),
            Outcome::NeedsInspection
        );
        drop(disk);
        let next_id = uuid::Uuid::new_v4().to_string();
        let next_workspace = parent.join(format!(".agbrio-update-{next_id}"));
        fs::create_dir(&next_workspace).unwrap();
        fs::set_permissions(&next_workspace, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(next_workspace.join("Agbrio.app")).unwrap();
        fs::write(next_workspace.join("Agbrio.app/fixture-identity"), b"third").unwrap();
        let (next, mut disk) = NativeDisk::attach(
            &parent,
            next_id,
            "0.1.1".into(),
            "0.1.2".into(),
            "b".repeat(64),
        )
        .unwrap();
        assert_eq!(
            apply(&next, &mut disk, || panic!("global pending intent")),
            Outcome::NeedsInspection
        );
        assert_eq!(
            fs::read(parent.join("Agbrio.app/fixture-identity")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(workspace.join("Agbrio.app/fixture-identity")).unwrap(),
            b"old"
        );
        drop(disk);
        let (parent, id, _) = fixture(&root_path, "foreign-lease");
        fs::write(parent.join(".agbrio-update.lock"), b"foreign lock").unwrap();
        assert!(attach(&parent, id).is_err());
        assert_eq!(
            fs::read(parent.join(".agbrio-update.lock")).unwrap(),
            b"foreign lock"
        );
        let (parent, id, workspace) = fixture(&root_path, "foreign-journal");
        let (binding, mut disk) = attach(&parent, id).unwrap();
        fs::write(workspace.join("journal.jsonl"), b"foreign receipt").unwrap();
        assert_eq!(
            apply(&binding, &mut disk, || panic!("foreign journal")),
            Outcome::NeedsInspection
        );
        assert_eq!(
            fs::read(workspace.join("journal.jsonl")).unwrap(),
            b"foreign receipt"
        );
        assert_eq!(
            fs::read(parent.join("Agbrio.app/fixture-identity")).unwrap(),
            b"old"
        );
        drop(disk);
        let (parent, id, workspace) = fixture(&root_path, "changed-workspace");
        let (binding, mut disk) = attach(&parent, id).unwrap();
        fs::rename(&workspace, parent.join("preserved-workspace")).unwrap();
        fs::create_dir(&workspace).unwrap();
        assert_eq!(
            apply(&binding, &mut disk, || panic!("foreign workspace")),
            Outcome::RejectedBeforeMutation
        );
        assert_eq!(
            fs::read(parent.join("Agbrio.app/fixture-identity")).unwrap(),
            b"old"
        );
        drop(disk);
        let (parent, id, workspace) = fixture(&root_path, "link");
        fs::rename(
            workspace.join("Agbrio.app"),
            workspace.join("preserved.app"),
        )
        .unwrap();
        symlink(
            workspace.join("preserved.app"),
            workspace.join("Agbrio.app"),
        )
        .unwrap();
        assert!(attach(&parent, id).is_err());
        assert_eq!(
            fs::read(workspace.join("preserved.app/fixture-identity")).unwrap(),
            b"new"
        );
        assert!(!parent.join(".agbrio-update.lock").exists());
        let (parent, id, workspace) = fixture(&root_path, "changed-current-before-swap");
        let (binding, mut disk) = attach(&parent, id).unwrap();
        disk.record(&binding, Phase::Intent).unwrap();
        fs::rename(
            parent.join("Agbrio.app"),
            parent.join("preserved-current.app"),
        )
        .unwrap();
        fs::create_dir(parent.join("Agbrio.app")).unwrap();
        fs::write(parent.join("Agbrio.app/fixture-identity"), b"foreign").unwrap();
        assert_eq!(disk.swap().unwrap_err(), "MAC_UPDATE_LAYOUT_CHANGED");
        assert_eq!(disk.swap().unwrap_err(), "MAC_UPDATE_SWAP_NOT_ADMITTED");
        assert_eq!(
            fs::read(parent.join("Agbrio.app/fixture-identity")).unwrap(),
            b"foreign"
        );
        assert_eq!(
            fs::read(parent.join("preserved-current.app/fixture-identity")).unwrap(),
            b"old"
        );
        assert_eq!(
            fs::read(workspace.join("Agbrio.app/fixture-identity")).unwrap(),
            b"new"
        );
        println!("\nMACOS_UPDATE_TRANSACTION_NATIVE_PASS");
    }
}
