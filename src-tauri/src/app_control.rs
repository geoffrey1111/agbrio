//! Application commands shared by authenticated HTTP, MCP and local maintenance.
//! This surface never calls a provider, changes approval policy or returns chat data.
use crate::{host_application::RouterCore, HostRuntime};
use serde_json::{json, Value};

pub(crate) const API_VERSION: u32 = 1;
pub(crate) fn version() -> &'static str {
    option_env!("VITE_AGBRIO_APP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub(crate) fn capabilities(scope: &str) -> Value {
    let instance = matches!(scope,"INSTANCE"|"OWNER");
    json!({
        "apiVersion":API_VERSION,"appVersion":version(),"scope":scope,
        "tools":crate::assistant_mcp::tools_for_scope(scope)["tools"],
        "discoveryOnly":true,"delegationRequired":true,
        "boards":{"publication":"OWNER_LOCAL_CLI_OR_DESKTOP","cliPath":if instance{std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("app-control/control.ps1").to_string_lossy().into_owned()))}else{None},"command":"app-control/control.ps1 -Command PublishBoard -Folder <absolute-folder>","manifest":"agbrio-board.json","schema":{"schemaVersion":1,"title":"My board","entry":"index.html","files":["index.html"]},"phone":"existing paired mobile origin; no separate domain setup","updates":"edit manifest-listed files in the registered folder","limits":{"files":256,"fileBytes":8388608,"totalBytes":33554432},"sandbox":"read-only presentation; no provider, permission or approval authority"},
        "applicationReads":instance,
        "http":if scope=="OWNER" {json!({"capabilities":"GET /v1/mobile/app/capabilities","diagnostics":"GET /v1/mobile/app/diagnostics","authentication":"existing owner authentication"})}else{Value::Null},
        "providers":{
            "PI":{"read":true,"mcpWrite":instance,"createChat":"REVIEWED_CREATE_CHAT","modelsTool":"agbrio_list_pi_models","identity":"pi:<nativeSessionId>","parentRelation":"REVIEWED_AGBRIO_DELEGATION","nativeCallerAttested":false,"directoryPermission":"OWNER_WORKSPACE_CEILING_MANAGER_TASK_SUBSET","modelSelection":["modelProvider","model","thinking"],"automaticRouting":"NOT_INSTALLED","defaultFollowUp":"NATIVE_STEER_WHEN_ACTIVE_SEND_WHEN_IDLE","implicitQueue":false,"terminal":"OWNER_AUTHORIZED_TASK_POLICY","administrator":"VERIFIED_WINDOWS_TOKEN_AND_UAC","resourceInheritance":"NATIVE_CONTEXT_AND_EXPLICIT_SKILL_ROOTS","taskScopeFields":["accessRoots","writable"],"directShellApi":false,"nativeTaskRestore":"EXACT_ORIGINAL_FILE_REVIEWED_ENABLE_WATCH","runtimeVerificationRequired":true},
            "CODEX":{"read":true,"createChat":"REVIEWED_CREATE_CHAT","createChatSettings":{"modelsTool":"agbrio_list_codex_models","fields":["model","effort","serviceTier"],"scope":"NEW_THREAD_ONLY","standardSpeed":"default","globalConfigWrite":false},"defaultFollowUp":"STEER_WHEN_ACTIVE_SEND_WHEN_IDLE","implicitQueue":false,"writes":"EXISTING_REVIEWED_ACTIONS","runtimeVerificationRequired":true},
            "CHATGPT":{"read":true,"writes":"EXISTING_REVIEWED_HANDOFF","accountSecurityStop":true},
            "CLAUDE_CODE":{"read":true,"writes":"OWNER_TEXT_AND_SELECTED_FILE_REFERENCES","mcpWrite":false,"attachments":"OWNER_LOCAL_FILE_REFERENCE","attachmentLimits":{"fileBytes":10485760,"totalBytes":41943040,"files":4},"goalControl":false}
        },
        "localMaintenance":{"transport":"WINDOWS_CURRENT_USER_NAMED_PIPE","commands":["STATUS","CAPABILITIES","DIAGNOSTICS","SHUTDOWN","PUBLISH_BOARD"],"start":"external local CLI","shutdownRequiresExactInstance":true,"remoteShutdown":false},
        "rules":["Discovery is not delegation or runtime readiness","Use stable native identities and existing review/receipt commands","After uncertain writes inspect the original receipt; never resend automatically","No shell, credentials, provider restart or permission changes"]
    })
}

pub(crate) fn diagnostics(core: &RouterCore, host: Option<&HostRuntime>) -> Result<Value, String> {
    let watches=core.store.codex_watches().map_err(|_|"APP_DIAGNOSTICS_STORE_UNAVAILABLE")?;
    let catalog=core.store.claude_catalog().map_err(|_|"APP_DIAGNOSTICS_STORE_UNAVAILABLE")?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    let codex=match core.session.try_lock() {
        Ok(s)=>json!({"observation":"AVAILABLE","connecting":s.connecting,"borrowed":s.codex_adapter_borrowed,"connected":s.adapter.as_ref().is_some_and(|a|!a.is_closed())}),
        Err(_)=>json!({"observation":"BUSY","connected":null})
    };
    let runtime=host.map(|h| {
        let web=h.web.status();
        json!({"instanceId":h.instance_id(),"pid":std::process::id(),"mobileListening":h.mobile_address().is_some(),
            "web":{"localReady":web.local_ready,"tunnelReady":web.tunnel_ready,"publicReady":web.public_ready,"checkedAt":web.checked_at}})
    });
    Ok(json!({"apiVersion":API_VERSION,"appVersion":version(),"checkedAt":now,"host":runtime,
        "store":{"readable":true},"codex":codex,
        "claude":{"plugin":crate::claude_code::plugin_versions(),"registeredSessions":catalog.sessions.len(),"livePublishers":catalog.sessions.iter().filter(|s|s.lease_until>now&&s.state!="SESSION_END").count(),"mcpWrite":false},
        "watches":{"total":watches.len(),"enabled":watches.iter().filter(|w|w.enabled).count()},
        "coverage":"LOCAL_OBSERVATION_ONLY","providerProbePerformed":false,
        "notice":"Counts and liveness do not prove end-to-end delivery, provider account readiness or user acceptance"}))
}
