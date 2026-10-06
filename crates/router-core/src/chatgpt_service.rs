//! One Host-owned ChatGPT production adapter. Configuration is a platform seam;
//! it is not another routing/state authority and never falls back to old carriers.
use crate::{
    browser_executor::ExecutorProcess,
    chatgpt::{ChatGPTAdapter, ExactReply, Receipt},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct ExecutorConfiguration {
    pub node: PathBuf,
    pub resources: PathBuf,
    pub data: PathBuf,
}
#[derive(Default)]
struct Session {
    configuration: Option<ExecutorConfiguration>,
    process: Option<Arc<ExecutorProcess>>,
}
#[derive(Default)]
pub struct ChatGPTService(Mutex<Session>, std::sync::atomic::AtomicBool);
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResource {
    pub resource_id: String,
    pub filename: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterializedResource {
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
impl ChatGPTService {
    pub fn configure(&self, configuration: ExecutorConfiguration) -> Result<(), String> {
        let mut session = self.0.lock().map_err(|_| "EXECUTOR_SESSION_UNAVAILABLE")?;
        if session.configuration.is_some() {
            return Err("EXECUTOR_ALREADY_CONFIGURED".into());
        }
        session.configuration = Some(configuration);
        Ok(())
    }
    fn process(&self) -> Result<Arc<ExecutorProcess>, String> {
        self.process_for(false)
    }
    fn process_for(&self, owner_security_action: bool) -> Result<Arc<ExecutorProcess>, String> {
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("EXECUTOR_STOPPED".into());
        }
        let mut session = self.0.lock().map_err(|_| "EXECUTOR_SESSION_UNAVAILABLE")?;
        if !owner_security_action && session
            .configuration
            .as_ref()
            .is_some_and(|config| config.data.join("browser-auth-required.json").exists())
        {
            return Err("AUTH_REQUIRED".into());
        }
        if let Some(process) = &session.process {
            return Ok(Arc::clone(process));
        }
        let config = session
            .configuration
            .as_ref()
            .ok_or("CHROMIUM_PACKAGE_UNAVAILABLE")?;
        let process = Arc::new(ExecutorProcess::start(
            &config.node,
            &config.resources.join("production-entry.mjs"),
            &config.resources,
            &config.data,
        )?);
        session.process = Some(Arc::clone(&process));
        Ok(process)
    }
    fn adapter(&self) -> Result<ChatGPTAdapter, String> {
        let process = self.process()?;
        let session = self.0.lock().map_err(|_| "EXECUTOR_SESSION_UNAVAILABLE")?;
        let data = &session
            .configuration
            .as_ref()
            .ok_or("CHROMIUM_PACKAGE_UNAVAILABLE")?
            .data;
        Ok(ChatGPTAdapter::resident(
            process,
            data.join("send-manifests"),
        ))
    }
    pub fn status(&self) -> &'static str {
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            return "STOPPED";
        }
        let Ok(session) = self.0.lock() else {
            return "UNAVAILABLE";
        };
        let Some(configuration) = &session.configuration else {
            return "UNAVAILABLE";
        };
        if configuration
            .data
            .join("browser-auth-required.json")
            .exists()
        {
            "AUTH_REQUIRED"
        } else if session.process.is_some() {
            "CONFIGURED"
        } else {
            "NOT_STARTED"
        }
    }
    pub fn open_for_authentication(&self) -> Result<Value, String> {
        self.process_for(true)?.request("open", json!({}))
    }
    pub fn current_binding(&self) -> Result<Value, String> {
        self.process()?.request("binding", json!({}))
    }
    pub fn verify_exact(&self, conversation_id: &str) -> Result<(), String> {
        let value = self
            .process()?
            .request("verify", json!({"conversationId": conversation_id}))?;
        if value["status"] == "EXACT_CONVERSATION" && value["conversationId"] == conversation_id {
            Ok(())
        } else {
            Err(value["status"]
                .as_str()
                .unwrap_or("EXACT_IDENTITY_UNPROVEN")
                .into())
        }
    }
    pub fn list_attachments(
        &self,
        conversation_id: &str,
        message_id: &str,
    ) -> Result<Vec<TerminalResource>, String> {
        let value = self.process()?.request(
            "list_attachments",
            json!({"conversationId": conversation_id, "messageId": message_id}),
        )?;
        if value["status"] != "RESOURCES"
            || value["conversationId"] != conversation_id
            || value["messageId"] != message_id
        {
            return Err(value["status"]
                .as_str()
                .unwrap_or("RESOURCE_EVIDENCE_INVALID")
                .into());
        }
        let resources: Vec<TerminalResource> = serde_json::from_value(value["resources"].clone())
            .map_err(|_| "RESOURCE_EVIDENCE_INVALID")?;
        let mut identities = std::collections::HashSet::new();
        for resource in &resources {
            if resource.resource_id.len() != 64
                || !resource
                    .resource_id
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || !identities.insert(resource.resource_id.clone())
                || !safe_filename(&resource.filename)
            {
                return Err("RESOURCE_EVIDENCE_INVALID".into());
            }
        }
        Ok(resources)
    }
    pub fn materialize_attachment(
        &self,
        conversation_id: &str,
        message_id: &str,
        resource_id: &str,
    ) -> Result<MaterializedResource, String> {
        let value = self.process()?.request("materialize_attachment", json!({"conversationId": conversation_id, "messageId": message_id, "resourceId": resource_id}))?;
        if value["status"] != "MATERIALIZED"
            || value["conversationId"] != conversation_id
            || value["messageId"] != message_id
            || value["resourceId"] != resource_id
        {
            return Err(value["status"]
                .as_str()
                .unwrap_or("RESOURCE_EVIDENCE_INVALID")
                .into());
        }
        let resource: MaterializedResource =
            serde_json::from_value(value).map_err(|_| "RESOURCE_EVIDENCE_INVALID")?;
        let session = self.0.lock().map_err(|_| "EXECUTOR_SESSION_UNAVAILABLE")?;
        let data = &session
            .configuration
            .as_ref()
            .ok_or("CHROMIUM_PACKAGE_UNAVAILABLE")?
            .data;
        let root = std::fs::canonicalize(data.join("materializations"))
            .map_err(|_| "RESOURCE_STORAGE_INVALID")?;
        let file = std::fs::canonicalize(&resource.path).map_err(|_| "RESOURCE_STORAGE_INVALID")?;
        if !file.starts_with(root)
            || !safe_filename(&resource.filename)
            || file.file_name().and_then(|name| name.to_str()) != Some(&resource.filename)
        {
            return Err("RESOURCE_STORAGE_INVALID".into());
        }
        let bytes = std::fs::read(file).map_err(|_| "RESOURCE_STORAGE_INVALID")?;
        if bytes.len() as u64 != resource.size
            || bytes.is_empty()
            || bytes.len() > 20 * 1024 * 1024
            || format!("{:x}", Sha256::digest(&bytes)) != resource.sha256
        {
            return Err("RESOURCE_HASH_MISMATCH".into());
        }
        Ok(resource)
    }
    pub fn owner_confirmed_authentication(&self) -> Result<Value, String> {
        // The Executor must prove native profile ownership handoff before
        // clearing the stop. Only explicit desktop/authenticated HTTP calls here.
        let result = self.process_for(true)?.request("owner_authentication_completed", json!({}))?;
        if result["status"] != "OWNER_CONFIRMATION_RECORDED" {
            return Err(result["code"].as_str().or(result["status"].as_str()).unwrap_or("AUTH_STOP_CLEAR_FAILED").into());
        }
        Ok(result)
    }
    pub fn observe_exact(&self, conversation_id: &str) -> Result<ExactReply, String> {
        self.adapter()?.observe_exact(conversation_id)
    }
    pub fn observe_passive(&self, conversation_id: &str) -> Result<ExactReply, String> {
        self.adapter()?.observe_passive(conversation_id)
    }
    pub fn submit_approved_exact(
        &self,
        conversation_id: &str,
        dispatch_id: &str,
        immutable_payload: &str,
    ) -> Result<Receipt, String> {
        self.adapter()?
            .submit_approved_exact(conversation_id, dispatch_id, immutable_payload)
    }
    pub fn receipt(&self, dispatch_id: &str) -> Result<Receipt, String> {
        self.adapter()?.receipt(dispatch_id)
    }
    pub fn submit_approved_with_attachments(
        &self,
        conversation_id: &str,
        dispatch_id: &str,
        immutable_payload: &str,
        attachments: Value,
    ) -> Result<Receipt, String> {
        self.adapter()?.submit_approved_with_attachments(
            conversation_id,
            dispatch_id,
            immutable_payload,
            attachments,
        )
    }
    pub fn shutdown(&self) {
        self.1.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut session) = self.0.lock() {
            if let Some(process) = session.process.take() {
                process.shutdown();
            }
        }
    }
}

fn safe_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename != "."
        && filename != ".."
        && !filename
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
}

#[cfg(test)]
mod auth_stop_tests {
    use super::*;
    #[test]
    fn durable_auth_stop_blocks_provider_spawn_and_failed_owner_handoff_cannot_clear_it() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../runtime/integrated-provider-router");
        std::fs::create_dir_all(&root).unwrap();
        let directory = tempfile::tempdir_in(root).unwrap();
        std::fs::write(
            directory.path().join("browser-auth-required.json"),
            br#"{"status":"AUTH_REQUIRED"}"#,
        )
        .unwrap();
        let service = ChatGPTService::default();
        service
            .configure(ExecutorConfiguration {
                node: directory.path().join("executor-must-never-spawn.exe"),
                resources: directory.path().join("missing-package"),
                data: directory.path().into(),
            })
            .unwrap();
        assert_eq!(service.status(), "AUTH_REQUIRED");
        assert_eq!(
            service.open_for_authentication().unwrap_err(),
            "EXECUTOR_START_FAILED"
        );
        assert_eq!(service.current_binding().unwrap_err(), "AUTH_REQUIRED");
        assert_eq!(
            service
                .verify_exact("00000000-0000-4000-8000-000000000001")
                .unwrap_err(),
            "AUTH_REQUIRED"
        );
        assert!(service.0.lock().unwrap().process.is_none());
        assert_eq!(service.owner_confirmed_authentication().unwrap_err(), "EXECUTOR_START_FAILED");
        assert_eq!(service.status(), "AUTH_REQUIRED");
        assert!(service.0.lock().unwrap().process.is_none());
    }

    /// Offline real packaged process/browser gate. Empty profile, no provider URL
    /// or account; explicit opt-in resources avoid ordinary suite browser launch.
    #[test]
    #[ignore]
    fn real_owner_security_recovery_empty_profile_no_provider() {
        let resources = std::fs::canonicalize(std::env::var("AIWR_RECOVERY_TEST_RESOURCES").unwrap()).unwrap();
        let node = std::fs::canonicalize(std::env::var("AIWR_RECOVERY_TEST_NODE").unwrap()).unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/integrated-provider-router");
        std::fs::create_dir_all(&root).unwrap();
        let directory = tempfile::Builder::new().prefix("recovery gate with spaces ").tempdir_in(root).unwrap();
        let marker = directory.path().join("browser-auth-required.json");
        let original = br#"{"status":"AUTH_REQUIRED","ownerActionRequired":true}"#;
        std::fs::write(&marker, original).unwrap();
        let service = ChatGPTService::default();
        let config = ExecutorConfiguration { resources, node, data: std::fs::canonicalize(directory.path()).unwrap() };
        service.configure(config.clone()).unwrap();
        assert_eq!(service.current_binding().unwrap_err(), "AUTH_REQUIRED");
        assert!(service.0.lock().unwrap().process.is_none());
        let opened = service.open_for_authentication().unwrap();
        assert_eq!(opened["mode"], "HUMAN_ONLY", "{opened}");
        assert!(matches!(opened["focus"].as_str(), Some("FOREGROUND" | "ATTENTION")), "{opened}");
        assert_eq!(service.open_for_authentication().unwrap()["reused"], true);
        assert_eq!(std::fs::read(&marker).unwrap(), original);
        assert_eq!(service.verify_exact("00000000-0000-4000-8000-000000000001").unwrap_err(), "AUTH_REQUIRED");
        assert_eq!(service.observe_passive("00000000-0000-4000-8000-000000000001").unwrap_err(), "AUTH_REQUIRED");
        assert_eq!(service.submit_approved_exact("00000000-0000-4000-8000-000000000001", "disposable", "no submit").err().unwrap(), "AUTH_REQUIRED");
        assert_eq!(service.list_attachments("disposable", "disposable").err().unwrap(), "AUTH_REQUIRED");
        let lease_path = directory.path().join("browser-profile/chatgpt/aiwr-browser-session.json");
        let lease: Value = serde_json::from_slice(&std::fs::read(&lease_path).unwrap()).unwrap();
        assert_eq!(lease["mode"], "HUMAN_ONLY");
        assert_eq!(lease["automated"], false);
        // Raw Executor operations must also be gated, even if Core is bypassed.
        let process = service.process_for(true).unwrap();
        for operation in ["health", "binding", "verify", "observe", "send", "list_attachments", "materialize_attachment"] {
            assert_eq!(process.request(operation, json!({})).unwrap()["status"], "AUTH_REQUIRED");
        }
        service.shutdown();
        assert!(lease_path.exists());
        assert!(marker.exists());
        let restarted = ChatGPTService::default();
        restarted.configure(config.clone()).unwrap();
        assert_eq!(restarted.open_for_authentication().unwrap()["reused"], true);
        let retained: Value = serde_json::from_slice(&std::fs::read(&lease_path).unwrap()).unwrap();
        assert_eq!(retained["pid"], lease["pid"]);
        assert_eq!(restarted.current_binding().unwrap_err(), "AUTH_REQUIRED");
        // Simulate the owner closing ONLY this disposable blank human window,
        // without authentication completion. No provider/profile is accessed.
        let mut close = std::process::Command::new(crate::chatgpt::node_path(&config.node).unwrap());
        close.args(["--input-type=module", "-e", r#"
          import {pathToFileURL} from 'node:url';
          import {readFile} from 'node:fs/promises';
          const {securityPlatform}=await import(pathToFileURL(process.argv[1]).href);
          const record=JSON.parse(await readFile(process.argv[2], 'utf8'));
          await securityPlatform.close(record);
          const end=Date.now()+10000;
          while ((await securityPlatform.inventory(record.profile)).length) {
            if(Date.now()>=end)throw new Error('FIXTURE_CLOSE_PENDING');
            await new Promise(resolve=>setTimeout(resolve,200));
          }
        "#]);
        close.arg(crate::chatgpt::node_path(&config.resources.join("security-browser.mjs")).unwrap());
        close.arg(crate::chatgpt::node_path(&lease_path).unwrap());
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            close.creation_flags(0x08000000);
        }
        let closed = close.output().unwrap();
        assert!(closed.status.success(), "{}", String::from_utf8_lossy(&closed.stderr));
        assert!(lease_path.exists());
        assert_eq!(std::fs::read(&marker).unwrap(), original);
        let reopened = restarted.open_for_authentication().unwrap();
        assert_eq!(reopened["status"], "OPEN");
        assert_eq!(reopened["reused"], false);
        let new_owner: Value = serde_json::from_slice(&std::fs::read(&lease_path).unwrap()).unwrap();
        assert_ne!(new_owner["pid"], retained["pid"]);
        assert_eq!(new_owner["profile"], retained["profile"]);
        assert_eq!(new_owner["binary"], retained["binary"]);
        assert_eq!(new_owner["automated"], false);
        assert_eq!(std::fs::read(&marker).unwrap(), original);
        assert_eq!(restarted.open_for_authentication().unwrap()["reused"], true);
        assert_eq!(restarted.current_binding().unwrap_err(), "AUTH_REQUIRED");
        let service = restarted;
        let process = service.process_for(true).unwrap();
        assert_eq!(service.owner_confirmed_authentication().unwrap()["status"], "OWNER_CONFIRMATION_RECORDED");
        assert!(!marker.exists());
        assert!(!lease_path.exists());
        // One local blank-page restart proves normal mode only AFTER the action.
        let health = process.request("health", json!({})).unwrap();
        assert_ne!(health["status"], "AUTH_REQUIRED");
        let resumed: Value = serde_json::from_slice(&std::fs::read(&lease_path).unwrap()).unwrap();
        assert_eq!(resumed["mode"], "AUTOMATED");
        service.shutdown();
        assert!(!lease_path.exists());
        assert_eq!(service.status(), "STOPPED");
    }
}
