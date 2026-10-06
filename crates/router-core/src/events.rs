use crate::codex::protocol::FeedEvent;

/// Host presentation only. Application facts use the separate native listener.
/// Implementations must not block the native reader.
pub trait EventSink: Send + Sync {
    fn wants_feed(&self) -> bool {
        true
    }
    fn feed(&self, event: FeedEvent);
    fn disconnected(&self);
    fn connected(&self) {}
    fn connection_failed(&self, _: &str) {
        self.disconnected();
    }
}

pub struct NullEventSink;
impl EventSink for NullEventSink {
    fn wants_feed(&self) -> bool {
        false
    }
    fn feed(&self, _: FeedEvent) {}
    fn disconnected(&self) {}
}
