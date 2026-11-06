//! Removes what retired features left under `AIWorkRouter`, once per launch.
//!
//! Only reproducible files on this fixed list go: a retired shared backend's own folder,
//! the downloaded browser and its extension copies, and the caches inside browser
//! profiles. Everything else in a profile stays, which is where sign-ins live: cookies,
//! saved passwords, local storage and `Local State`. A failure leaves files for the next
//! launch. Links are never followed.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Chromium rebuilds these. A sign-in is never stored in any of them.
const PROFILE_CACHES: [&str; 13] = [
    "Default/Cache",
    "Default/Code Cache",
    "Default/DawnGraphiteCache",
    "Default/DawnWebGPUCache",
    "Default/GPUCache",
    "BrowserMetrics",
    "Crashpad",
    "GPUPersistentCache",
    "GraphiteDawnCache",
    "GrShaderCache",
    "ShaderCache",
    "component_crx_cache",
    "optimization_guide_model_store",
];

pub(super) fn run(root: &Path) {
    remove_retired_shared(&root.join("codex-shared"));
    let runtime = root.join("browser-runtime");
    remove_children(&runtime, &["extension", "runtime-data-", "chromium-"]);
    trim_profiles(&runtime);
    let executor = root.join("browser-executor");
    remove_children(&executor.join("browser-runtime"), &["chromium-"]);
    remove(&executor.join("materializations"));
    trim_profile(&executor.join("browser-profile"));
    trim_profile(&root.join("playwright-chatgpt-profile"));
}

/// The settings file goes last, so an interrupted removal is finished by the next launch.
fn remove_retired_shared(directory: &Path) {
    let settings = directory.join("settings.json");
    let retired = fs::read(&settings).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).is_some_and(|saved| saved["enabled"] == false && saved["phase"] == "DISABLED");
    if !retired {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else { return };
    let mut complete = true;
    for entry in entries.flatten() {
        if entry.file_name() != "settings.json" {
            complete &= remove(&entry.path());
        }
    }
    if complete && remove(&settings) {
        let _ = fs::remove_dir(directory);
    }
}

fn remove_children(directory: &Path, prefixes: &[&str]) {
    for path in children(directory) {
        if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| prefixes.iter().any(|prefix| name.starts_with(prefix))) {
            remove(&path);
        }
    }
}

fn trim_profiles(directory: &Path) {
    for path in children(directory) {
        if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("profile")) {
            trim_profile(&path);
        }
    }
}

fn trim_profile(profile: &Path) {
    if !is_plain_directory(profile) {
        return;
    }
    for cache in PROFILE_CACHES {
        remove(&profile.join(cache));
    }
}

fn children(directory: &Path) -> Vec<PathBuf> {
    if !is_plain_directory(directory) {
        return Vec::new();
    }
    fs::read_dir(directory).map(|entries| entries.flatten().map(|entry| entry.path()).collect()).unwrap_or_default()
}

fn is_plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

/// True when nothing remains. A link is removed itself, never what it points at.
fn remove(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else { return true };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).is_ok()
    } else if metadata.file_type().is_symlink() && metadata.is_dir() {
        fs::remove_dir(path).is_ok()
    } else {
        fs::remove_file(path).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    fn shared(root: &Path, settings: &str) -> PathBuf {
        let directory = root.join("codex-shared");
        write(&directory.join("native/hash/codex.exe"));
        write(&directory.join("runtime/node.exe"));
        fs::write(directory.join("settings.json"), settings).unwrap();
        directory
    }

    #[test]
    fn a_disabled_shared_backend_is_removed_completely() {
        let root = tempfile::tempdir().unwrap();
        let directory = shared(root.path(), r#"{"enabled":false,"phase":"DISABLED"}"#);
        run(root.path());
        assert!(!directory.exists());
    }

    #[test]
    fn anything_else_about_the_shared_backend_keeps_its_folder() {
        for settings in [r#"{"enabled":true,"phase":"ACTIVE"}"#, r#"{"enabled":false,"phase":"RECOVERY_REQUIRED"}"#, r#"{"enabled":false}"#, "not json"] {
            let root = tempfile::tempdir().unwrap();
            let directory = shared(root.path(), settings);
            run(root.path());
            assert!(directory.join("runtime/node.exe").exists(), "{settings}");
            assert!(directory.join("settings.json").exists(), "{settings}");
        }
        let root = tempfile::tempdir().unwrap();
        write(&root.path().join("codex-shared/runtime/node.exe"));
        run(root.path());
        assert!(root.path().join("codex-shared/runtime/node.exe").exists(), "no settings proves nothing about the certificate");
    }

    #[test]
    fn sign_ins_stay_and_only_listed_caches_go() {
        let root = tempfile::tempdir().unwrap();
        for profile in ["browser-runtime/profile", "browser-runtime/profile-chrome-v1", "browser-executor/browser-profile", "playwright-chatgpt-profile"] {
            let profile = root.path().join(profile);
            let kept = ["Local State", "Default/Cookies", "Default/Network/Cookies", "Default/Login Data", "Default/Web Data", "Default/Preferences", "Default/Local Storage/leveldb/000.log", "Default/IndexedDB/a/000.log", "Default/Session Storage/000.log"];
            for name in kept {
                write(&profile.join(name));
            }
            for cache in PROFILE_CACHES {
                write(&profile.join(cache).join("data_0"));
            }
            run(root.path());
            for name in kept {
                assert!(profile.join(name).exists(), "{name}");
            }
            for cache in PROFILE_CACHES {
                assert!(!profile.join(cache).exists(), "{cache}");
            }
        }
    }

    #[test]
    fn the_downloaded_browser_and_its_copies_go() {
        let root = tempfile::tempdir().unwrap();
        for name in ["browser-runtime/extension/a.js", "browser-runtime/extension-keyless-v2/a.js", "browser-runtime/runtime-data-keyless-v1/a", "browser-runtime/chromium-123/chrome.exe", "browser-executor/browser-runtime/chromium-123/chrome.exe", "browser-executor/materializations/id/a.bin"] {
            write(&root.path().join(name));
        }
        write(&root.path().join("browser-executor/browser-receipts/id"));
        write(&root.path().join("browser-executor/owned-profile-location.json"));
        write(&root.path().join("data/router.db"));
        write(&root.path().join("chatgpt-bridge/data/a"));
        run(root.path());
        for gone in ["browser-runtime/extension", "browser-runtime/extension-keyless-v2", "browser-runtime/runtime-data-keyless-v1", "browser-runtime/chromium-123", "browser-executor/browser-runtime/chromium-123", "browser-executor/materializations"] {
            assert!(!root.path().join(gone).exists(), "{gone}");
        }
        for kept in ["browser-executor/browser-receipts/id", "browser-executor/owned-profile-location.json", "data/router.db", "chatgpt-bridge/data/a"] {
            assert!(root.path().join(kept).exists(), "{kept}");
        }
    }

    /// Needs a privilege Windows only grants some accounts, so it is skipped without one.
    #[test]
    fn a_link_is_never_followed() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("Cache/data_0"));
        let profile = root.path().join("playwright-chatgpt-profile");
        fs::create_dir_all(profile.join("Default")).unwrap();
        if std::os::windows::fs::symlink_dir(outside.path().join("Cache"), profile.join("Default/Cache")).is_err() {
            return;
        }
        run(root.path());
        assert!(outside.path().join("Cache/data_0").exists());
    }
}
