//! Local-owner directory grants. These checks are not a Windows sandbox.
use super::session::plain;
use std::path::{Component, Path, PathBuf};

pub(super) fn canonical_directory(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if !path.is_absolute()
        || value.len() > 4096
        || value.starts_with(['\\', '/'])
        || value.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    '\u{a0}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
                )
        })
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("PI_DIRECTORY_INVALID".into());
    }
    // Inspect every ancestor before canonicalization, so a junction is not
    // silently followed into a different grant or network location.
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(ancestor).map_err(|_| "PI_DIRECTORY_UNAVAILABLE")?;
        if metadata.file_type().is_symlink() {
            return Err("PI_DIRECTORY_LINK_DENIED".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("PI_DIRECTORY_LINK_DENIED".into());
            }
        }
    }
    if !path.is_dir() {
        return Err("PI_DIRECTORY_UNAVAILABLE".into());
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| "PI_DIRECTORY_UNAVAILABLE")?;
    Ok(PathBuf::from(plain(&canonical)))
}

pub(crate) fn authorize_root(value: &str) -> Result<String, String> {
    let canonical = canonical_directory(value)?;
    if canonical.parent().is_none() || canonical.file_name().is_none() {
        return Err("PI_SCOPE_ROOT_TOO_BROAD".into());
    }
    for key in [
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "SystemRoot",
        "ProgramData",
    ] {
        if let Some(protected) = std::env::var_os(key) {
            let protected = PathBuf::from(protected);
            // User profile itself/ancestors are too broad. Descendant projects
            // may be granted; app/system/credential trees remain forbidden.
            if protected.starts_with(&canonical)
                || (key != "USERPROFILE" && canonical.starts_with(&protected))
            {
                return Err("PI_SCOPE_PROTECTED_DIRECTORY".into());
            }
        }
    }
    if canonical.components().any(|c| {
        c.as_os_str().to_str().is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                ".pi" | ".claude" | ".codex" | ".ssh" | ".aws" | ".azure" | ".git" | ".agents"
            )
        })
    }) {
        return Err("PI_SCOPE_PROTECTED_DIRECTORY".into());
    }
    Ok(plain(&canonical))
}

pub(super) fn admitted_directory(scope: &str, value: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(authorize_root(scope)?);
    let directory = PathBuf::from(authorize_root(value)?);
    if directory.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(".agents"))
    }) {
        return Err("PI_SCOPE_PROTECTED_DIRECTORY".into());
    }
    if !directory.starts_with(root) {
        return Err("PI_DIRECTORY_OUT_OF_SCOPE".into());
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grants_are_canonical_exact_and_cannot_follow_junctions() {
        let base =
            std::fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
                .unwrap();
        let base = PathBuf::from(plain(&base));
        let root = tempfile::tempdir_in(&base).unwrap();
        let outside = tempfile::tempdir_in(&base).unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        assert!(admitted_directory(&plain(root.path()), &plain(&child)).is_ok());
        assert!(admitted_directory(&plain(root.path()), &plain(outside.path())).is_err());
        #[cfg(windows)]
        {
            let link = root.path().join("linked");
            std::os::windows::fs::symlink_dir(outside.path(), &link).unwrap();
            assert!(admitted_directory(&plain(root.path()), &plain(&link)).is_err());
        }
        assert!(canonical_directory(&format!("{}/../escape", plain(root.path()))).is_err());
    }
}
