//! Profile ownership is separate from durable Workstream dispatch locks.
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

pub struct PreviewProfile {
    root: PathBuf,
    _lock: File,
}
impl PreviewProfile {
    /// Explicit isolated preview only. Never silently opens the desktop data directory.
    pub fn acquire(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref();
        if !root.is_absolute() || root.file_name().and_then(|v| v.to_str()) != Some("mcp-preview") {
            return Err("PREVIEW_PROFILE_REQUIRED".into());
        }
        for ancestor in root.ancestors() {
            if ancestor.exists() {
                reject_link(ancestor)?;
            }
        }
        fs::create_dir_all(root).map_err(|_| "PROFILE_CREATE_FAILED")?;
        let root = fs::canonicalize(root).map_err(|_| "PROFILE_CANONICAL_FAILED")?;
        for name in ["writer.lock", "router.db"] {
            let path = root.join(name);
            if path.exists() {
                reject_link(&path)?;
            }
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("writer.lock"))
            .map_err(|_| "PROFILE_LOCK_OPEN_FAILED")?;
        lock.try_lock().map_err(|_| "PROFILE_ALREADY_OWNED")?;
        Ok(Self { root, _lock: lock })
    }
    pub fn database_path(&self) -> PathBuf {
        self.root.join("router.db")
    }
    pub(crate) fn backup_path(&self, version: u8) -> PathBuf {
        self.root
            .join(format!("pre-{version:03}-{}.db", uuid::Uuid::new_v4()))
    }
    pub(crate) fn migration_marker_path(&self) -> PathBuf {
        self.root.join("schema-maintenance.json")
    }
}
fn reject_link(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "PROFILE_METADATA_FAILED")?;
    if meta.file_type().is_symlink() {
        return Err("PROFILE_LINK_REJECTED".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("PROFILE_REPARSE_REJECTED".into());
        }
    }
    Ok(())
}
