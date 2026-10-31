//! PiAdapter owns the native process/protocol. No raw RPC or argv enters Domain/UI.
pub(crate) mod application;
#[cfg(test)]
mod live_tests;
pub(crate) mod media;
mod permissions;
pub(crate) mod workspaces;
pub(crate) mod presentation;
mod projection;
#[cfg(test)]
mod qa_host_tests;
#[cfg(test)]
mod workspace_tests;
mod service;
mod session;
pub(crate) use service::PiService;
mod history;
mod settings;
mod transport;
mod windows_job;
mod elevation;
mod cohort;
mod resources;
mod integrations;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
pub(crate) struct Installation {
    pub node: PathBuf,
    pub cli: PathBuf,
    pub version: String,
}
pub(crate) fn installation() -> Result<Installation, String> {
    let node = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .ok_or("PI_NODE_NOT_FOUND")?
        .join("nodejs/node.exe");
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("PI_NOT_INSTALLED")?
        .join("npm/node_modules/@earendil-works/pi-coding-agent");
    let manifest = root.join("package.json");
    let meta = std::fs::metadata(&manifest).map_err(|_| "PI_NOT_INSTALLED")?;
    if meta.len() > 65536 {
        return Err("PI_INSTALLATION_INVALID".into());
    }
    let pkg: Value =
        serde_json::from_slice(&std::fs::read(manifest).map_err(|_| "PI_NOT_INSTALLED")?)
            .map_err(|_| "PI_INSTALLATION_INVALID")?;
    if pkg["name"] != "@earendil-works/pi-coding-agent" || !node.is_file() {
        return Err("PI_INSTALLATION_INVALID".into());
    }
    let version = pkg["version"]
        .as_str()
        .ok_or("PI_INSTALLATION_INVALID")?
        .to_owned();
    let cli =
        std::fs::canonicalize(root.join("dist/bundle/cli.js")).map_err(|_| "PI_NOT_INSTALLED")?;
    let boundary = std::fs::canonicalize(root).map_err(|_| "PI_INSTALLATION_INVALID")?;
    if !cli.starts_with(boundary) {
        return Err("PI_INSTALLATION_INVALID".into());
    }
    Ok(Installation { node, cli, version })
}
fn args(i: &Installation, sessions: &std::path::Path) -> Vec<String> {
    let path = i.cli.to_string_lossy();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    vec![
        path.to_owned(),
        "--mode".into(),
        "rpc".into(),
        "--session-dir".into(),
        sessions.to_string_lossy().into_owned(),
        "--no-extensions".into(),
        "--no-mcp".into(),
        "--no-skills".into(),
        "--no-prompt-templates".into(),
        "--no-themes".into(),
        "--no-context-files".into(),
        "--no-tools".into(),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit existing Pi metadata-only handshake; no model prompt/key reads"]
    fn installed_pi_metadata_handshake() {
        let installation = installation().unwrap();
        let root =
            std::path::PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
        assert!(root.is_absolute() && root.is_dir());
        let sessions = root.join("metadata-sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let rpc = transport::Rpc::start(
            &installation.node,
            &args(&installation, &sessions),
            &root,
            root.to_str().unwrap(),
            false,
            Arc::new(|_| {}),
        )
        .unwrap();
        let state = rpc
            .request(
                "metadata-state",
                json!({"type":"get_state"}),
                Duration::from_secs(15),
            )
            .unwrap();
        assert_eq!(state["success"], true);
        assert!(state["data"]["sessionId"].is_string());
        let models = rpc
            .request(
                "metadata-models",
                json!({"type":"get_available_models"}),
                Duration::from_secs(15),
            )
            .unwrap();
        assert_eq!(models["success"], true);
        let count = models["data"]["models"].as_array().unwrap().len();
        assert!(count > 0);
        let levels = rpc
            .request(
                "metadata-levels",
                json!({"type":"get_available_thinking_levels"}),
                Duration::from_secs(15),
            )
            .unwrap();
        assert_eq!(levels["success"], true);
        eprintln!(
            "Pi {} metadata handshake: {} models; defaults provider={}, model={}, thinking={}",
            installation.version,
            count,
            state["data"]["model"]["provider"],
            state["data"]["model"]["id"],
            state["data"]["thinkingLevel"]
        );
        rpc.close(Duration::from_secs(5)).unwrap();
    }
}

pub(crate) fn prepare_administrator()->Result<Value,String>{cohort::prepare()}
