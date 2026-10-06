#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Retained registrations may still invoke these old spellings. Recognize them
// only to exit before startup; this executable has no Native Messaging writer.
const NATIVE_MESSAGING_CONNECTOR_ORIGIN: &str =
    "chrome-extension://hbbmojhcijkbddnidnobakilcloepadn";
const NATIVE_MESSAGING_CONNECTOR_MANIFEST_ORIGIN: &str =
    "chrome-extension://hbbmojhcijkbddnidnobakilcloepadn/";

fn is_retired_native_transport_launch(arguments: impl IntoIterator<Item = String>) -> bool {
    arguments.into_iter().any(|argument| {
        // This compatibility rejection neither changes browser registration
        // nor opens a desktop/browser fallback.
        argument == "--chatgpt-extension-bridge"
            || argument == NATIVE_MESSAGING_CONNECTOR_ORIGIN
            || argument == NATIVE_MESSAGING_CONNECTOR_MANIFEST_ORIGIN
    })
}

fn main() {
    if is_retired_native_transport_launch(std::env::args()) {
        // Reject a retained old registration; never dispatch its protocol or
        // silently launch a replacement provider/desktop writer.
        eprintln!("LEGACY_CHATGPT_TRANSPORT_RETIRED");
        return;
    }
    ai_work_router_lib::run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_retired_connector_invocations_for_rejection_before_startup() {
        assert!(is_retired_native_transport_launch([
            "ai-work-router.exe".to_owned(),
            NATIVE_MESSAGING_CONNECTOR_ORIGIN.to_owned(),
        ]));
        assert!(is_retired_native_transport_launch([
            "ai-work-router.exe".to_owned(),
            NATIVE_MESSAGING_CONNECTOR_MANIFEST_ORIGIN.to_owned(),
        ]));
        assert!(is_retired_native_transport_launch([
            "ai-work-router.exe".to_owned(),
            "--chatgpt-extension-bridge".to_owned(),
        ]));
        assert!(!is_retired_native_transport_launch([
            "ai-work-router.exe".to_owned(),
            "chrome-extension://other-extension/".to_owned(),
        ]));
    }
}
