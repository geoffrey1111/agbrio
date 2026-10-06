//! Bounded owned-adapter events only; not a task queue or replay bus.
use super::preparation::PreparationGuard;
use serde_json::Value;
use std::sync::{mpsc, Arc, Mutex};

const MAX_COUNT: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;
pub struct CriticalSender {
    sender: mpsc::SyncSender<(Value, usize)>,
    bytes: Arc<Mutex<usize>>,
    guard: PreparationGuard,
}
pub struct CriticalReceiver {
    receiver: mpsc::Receiver<(Value, usize)>,
    bytes: Arc<Mutex<usize>>,
}
pub fn channel(guard: PreparationGuard) -> (CriticalSender, CriticalReceiver) {
    let (sender, receiver) = mpsc::sync_channel(MAX_COUNT);
    let bytes = Arc::new(Mutex::new(0));
    (
        CriticalSender {
            sender,
            bytes: bytes.clone(),
            guard,
        },
        CriticalReceiver { receiver, bytes },
    )
}
impl CriticalSender {
    pub fn observe(&self, event: &Value) {
        // The stdout reader has already run its safety guard before this callback.
        let method = event.get("method").and_then(Value::as_str).unwrap_or("");
        if event.get("id").is_none()
            && !matches!(
                method,
                "turn/started"
                    | "turn/completed"
                    | "item/completed"
                    | "serverRequest/resolved"
                    | "thread/status/changed"
                    | "thread/goal/updated"
                    | "thread/goal/cleared"
            )
        {
            return;
        }
        let size = event.to_string().len();
        let Ok(mut total) = self.bytes.lock() else {
            self.guard.fail("OBSERVATION_FAILED");
            return;
        };
        if size > 8 * 1024 * 1024 || *total + size > MAX_BYTES {
            self.guard.fail("CRITICAL_QUEUE_BYTES");
            return;
        }
        *total += size;
        if self.sender.try_send((event.clone(), size)).is_err() {
            *total -= size;
            self.guard.fail("CRITICAL_QUEUE_OVERFLOW");
        }
    }
}
impl CriticalReceiver {
    pub fn try_next(&self) -> Result<Option<Value>, String> {
        match self.receiver.try_recv() {
            Ok((event, size)) => {
                let mut bytes = self.bytes.lock().map_err(|_| "OBSERVATION_FAILED")?;
                *bytes = bytes.checked_sub(size).ok_or("OBSERVATION_FAILED")?;
                Ok(Some(event))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("OBSERVATION_CLOSED".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn count_overflow_stops_even_ordinary_rpc_guard_without_dropping_failure() {
        let guard = PreparationGuard::default();
        let (tx, rx) = channel(guard.clone());
        for _ in 0..MAX_COUNT {
            tx.observe(&json!({"method":"thread/goal/cleared"}));
        }
        guard.check().unwrap();
        tx.observe(&json!({"method":"thread/goal/cleared"}));
        assert!(guard.check().is_err());
        while rx.try_next().unwrap().is_some() {}
        assert!(guard.check().is_err());
    }
    #[test]
    fn byte_budget_and_noncritical_execution_are_independently_guarded() {
        let guard = PreparationGuard::default();
        let (tx, _rx) = channel(guard.clone());
        let event = json!({"method":"thread/goal/cleared","fixture":"x".repeat(6*1024*1024)});
        tx.observe(&event);
        tx.observe(&event);
        guard.check().unwrap();
        tx.observe(&event);
        assert!(guard.check().is_err());
        let guard = PreparationGuard::default();
        let (tx, rx) = channel(guard.clone());
        guard.observe(&json!({"method":"item/agentMessage/delta"}));
        tx.observe(&json!({"method":"item/agentMessage/delta"}));
        assert!(guard.check().is_err());
        assert!(rx.try_next().unwrap().is_none());
    }
}
