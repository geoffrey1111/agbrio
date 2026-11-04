//! Production Mac gate remains closed until stage-4 real-package validation.
//! tools/shared-codex-macos contains the internal app-server lifecycle candidate;
//! its fixture readiness must never enable this product entry or Desktop status.
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub enabled: bool,
    pub ready: bool,
    pub desktop_connected: bool,
    pub state: String,
    pub update_pending: bool,
    pub launch_as_administrator: bool,
    pub current_administrator: Option<bool>,
    pub launch_progress: Option<serde_json::Value>,
}

pub(crate) fn configure(_: PathBuf, _: PathBuf) {}
pub(crate) fn enabled() -> bool { false }
pub(crate) fn transport_command() -> Result<Option<Command>, String> { Ok(None) }
pub(crate) fn status() -> Status {
    Status { enabled: false, ready: false, desktop_connected: false,
        state: "UNSUPPORTED".into(), update_pending: false,
        launch_as_administrator: false, current_administrator: None, launch_progress: None }
}
pub(crate) fn setup() -> Result<Status, String> { Err(super::platform::UNSUPPORTED.into()) }
pub(crate) fn launch() -> Result<Status, String> { setup() }
pub(crate) fn disable() -> Result<Status, String> { setup() }
pub(crate) fn set_launch_administrator(_: bool) -> Result<Status, String> { setup() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_shared_controls_never_report_a_ready_backend() {
        assert_eq!(status().state, "UNSUPPORTED");
        assert!(!enabled());
        assert!(transport_command().unwrap().is_none());
        assert!(setup().is_err());
        assert!(launch().is_err());
        assert!(disable().is_err());
        assert!(set_launch_administrator(true).is_err());
    }
}
