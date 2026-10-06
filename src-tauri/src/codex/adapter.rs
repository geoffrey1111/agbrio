#[cfg(test)]
use super::protocol::FeedEvent;
pub use router_core::codex::adapter::WRITE_READINESS_TIMEOUT;
use router_core::codex::platform::codex_app_server_command;
use router_core::{codex::adapter::CodexAdapter as CoreAdapter, events::EventSink};
use serde_json::Value;
use std::sync::Arc;

/// The real Core integration harness deliberately has no Tauri window. Its
/// disposable adapter session still uses the same official app-server process,
/// but provider events are asserted from the persisted test store instead of
/// being projected to a WebView event channel.
#[cfg(test)]
struct ValidationEventSink;
#[cfg(test)]
impl EventSink for ValidationEventSink {
    fn feed(&self, _: FeedEvent) {}
    fn disconnected(&self) {}
}

pub struct CodexAdapter(CoreAdapter);
impl CodexAdapter {
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
    #[cfg(debug_assertions)]
    pub fn request_counts(&self) -> std::collections::HashMap<String, u64> {
        self.0.request_counts()
    }
    pub fn start(
        sink: Arc<dyn EventSink>,
        listener: Arc<dyn Fn(&Value) + Send + Sync>,
    ) -> Result<Self, String> {
        match crate::shared_codex::transport_command()? {
            Some(command)=>CoreAdapter::start_shared_transport(sink,listener,command).map(Self),
            None=>CoreAdapter::start(sink,listener,codex_app_server_command()).map(Self)
        }
    }
    #[cfg(test)]
    pub(crate) fn start_shared_validation(command:std::process::Command,listener:Arc<dyn Fn(&Value)+Send+Sync>)->Result<Self,String>{CoreAdapter::start_shared_transport(Arc::new(ValidationEventSink),listener,command).map(Self)}
    #[cfg(test)]
    pub(crate) fn start_validation(
        listener: Arc<dyn Fn(&Value) + Send + Sync>,
    ) -> Result<Self, String> {
        CoreAdapter::start(
            Arc::new(ValidationEventSink),
            listener,
            codex_app_server_command(),
        )
        .map(Self)
    }
}

impl std::ops::Deref for CodexAdapter {
    type Target = CoreAdapter;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for CodexAdapter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
