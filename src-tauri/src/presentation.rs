//! Desktop projection only. Native protocol and Host services receive EventSink.
use router_core::{codex::protocol::FeedEvent, events::EventSink};
use tauri::{AppHandle, Emitter};

pub(crate) struct TauriEventSink(pub(crate) AppHandle);
impl EventSink for TauriEventSink {
    fn feed(&self, event: FeedEvent) {
        let _ = self.0.emit("codex-event", event);
    }
    fn disconnected(&self) {
        self.connection_failed("Codex backend disconnected");
    }
    fn connected(&self) {
        let _ = self.0.emit("codex-backend-connected", ());
    }
    fn connection_failed(&self, error: &str) {
        let _ = self.0.emit("codex-backend-disconnected", error);
    }
}
