//! Registered filesystem identity; never accepts a model-selected cwd.
use crate::identity::length_prefixed_hash;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct RootIdentity {
    pub canonical: PathBuf,
    pub hash: String,
}
impl RootIdentity {
    pub fn inspect(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("ROOT_CHANGED".into());
        }
        for p in path.ancestors() {
            let meta = fs::symlink_metadata(p).map_err(|_| "ROOT_CHANGED")?;
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err("ROOT_CHANGED".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("ROOT_CHANGED".into());
                }
            }
        }
        let canonical = fs::canonicalize(path).map_err(|_| "ROOT_CHANGED")?;
        let identity = directory_id(&canonical)?;
        let name = canonical.to_str().ok_or("ROOT_CHANGED")?;
        let hash = length_prefixed_hash(&[b"registered-root-v1", name.as_bytes(), &identity]);
        Ok(Self { canonical, hash })
    }
    pub fn verify(path: &Path, expected_hash: &str) -> Result<Self, String> {
        let root = Self::inspect(path)?;
        if root.hash != expected_hash {
            return Err("ROOT_CHANGED".into());
        }
        Ok(root)
    }
}
#[cfg(windows)]
fn directory_id(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
    };
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|_| "ROOT_CHANGED")?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // File owns the live directory handle for the duration of this OS query.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err("ROOT_CHANGED".into());
    }
    let mut value = Vec::new();
    value.extend(info.dwVolumeSerialNumber.to_be_bytes());
    value.extend(info.nFileIndexHigh.to_be_bytes());
    value.extend(info.nFileIndexLow.to_be_bytes());
    Ok(value)
}
#[cfg(unix)]
fn directory_id(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::metadata(path).map_err(|_| "ROOT_CHANGED")?;
    let mut value = meta.dev().to_be_bytes().to_vec();
    value.extend(meta.ino().to_be_bytes());
    Ok(value)
}
#[cfg(not(any(windows, unix)))]
fn directory_id(_path: &Path) -> Result<Vec<u8>, String> {
    Err("ROOT_IDENTITY_UNSUPPORTED".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_extended_and_dos_paths_verify_the_same_directory_object() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let original = RootIdentity::inspect(&root).unwrap();
        let extended = original.canonical.to_str().unwrap();
        let dos = extended.strip_prefix("\\\\?\\").unwrap();
        let verified = RootIdentity::verify(Path::new(dos), &original.hash).unwrap();
        assert_eq!(verified.canonical, original.canonical);
        assert!(RootIdentity::verify(temp.path(), &original.hash).is_err());
    }
    #[test]
    fn same_spelling_replaced_directory_is_not_same_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let original = RootIdentity::inspect(&root).unwrap();
        RootIdentity::verify(&root, &original.hash).unwrap();
        fs::rename(&root, temp.path().join("preserved-original")).unwrap();
        fs::create_dir(&root).unwrap();
        assert!(RootIdentity::verify(&root, &original.hash).is_err());
        assert!(RootIdentity::inspect(Path::new("relative")).is_err());
    }
}
