//! Filesystem/process infrastructure only; provider routing never depends on it.
use std::path::{Path, PathBuf};

/// Parent of the historical AIWorkRouter directory. Do not synthesize Windows
/// environment variables on Unix or fall back to the process working directory.
pub fn local_data_base() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base: Option<PathBuf> = None;
    absolute_base(base.as_deref())
}

fn absolute_base(base: Option<&Path>) -> Result<PathBuf, String> {
    base.filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
        .ok_or_else(|| "LOCAL_DATA_DIRECTORY_UNAVAILABLE".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_relative_base_never_uses_working_directory() {
        assert!(absolute_base(None).is_err());
        assert!(absolute_base(Some(Path::new("relative"))).is_err());
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(absolute_base(Some(directory.path())).unwrap(), directory.path());
    }
}
