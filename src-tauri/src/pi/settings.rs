//! Strict session-local selection. No global configuration setters or AUTO.
use super::transport::Rpc;
use router_core::store::pi::Selection;
use serde_json::{json, Value};
use std::time::Duration;

pub(super) fn call(rpc: &Rpc, command: Value) -> Result<Value, String> {
    let expected = command["type"]
        .as_str()
        .ok_or("PI_COMMAND_INVALID")?
        .to_owned();
    let reply = rpc.request(
        &uuid::Uuid::new_v4().to_string(),
        command,
        Duration::from_secs(20),
    )?;
    if reply["command"] != expected || reply["success"] != true {
        // Native errors can contain credentials or private paths. Expose a stable
        // local error; never blindly relay an entire provider response.
        return Err(format!("PI_{}_REJECTED", expected.to_ascii_uppercase()));
    }
    Ok(reply["data"].clone())
}

pub(super) fn actual(state: &Value) -> Result<Selection, String> {
    let field = |v: &Value| {
        v.as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or("PI_SETTINGS_UNCONFIRMED".to_string())
    };
    Ok(Selection {
        provider: field(&state["model"]["provider"])?,
        model: field(&state["model"]["id"])?,
        thinking: field(&state["thinkingLevel"])?,
    })
}

pub(super) fn handshake(
    rpc: &Rpc,
    requested: Option<&Selection>,
) -> Result<(Value, Selection, Value), String> {
    handshake_with(|command| call(rpc, command), requested)
}

fn handshake_with(
    mut invoke: impl FnMut(Value) -> Result<Value, String>,
    requested: Option<&Selection>,
) -> Result<(Value, Selection, Value), String> {
    let catalog = invoke(json!({"type":"get_available_models"}))?;
    let models = catalog["models"]
        .as_array()
        .ok_or("PI_MODEL_CATALOG_INVALID")?;
    if let Some(selection) = requested {
        if !models
            .iter()
            .any(|v| v["provider"] == selection.provider && v["id"] == selection.model)
        {
            return Err("PI_MODEL_NOT_AVAILABLE".into());
        }
        invoke(
            json!({"type":"set_model","provider":selection.provider,"modelId":selection.model}),
        )?;
    }
    let levels = invoke(json!({"type":"get_available_thinking_levels"}))?;
    let supported = levels["levels"]
        .as_array()
        .ok_or("PI_THINKING_CATALOG_INVALID")?;
    if let Some(selection) = requested {
        if !supported
            .iter()
            .any(|v| v.as_str() == Some(&selection.thinking))
        {
            return Err("PI_THINKING_NOT_AVAILABLE".into());
        }
        invoke(json!({"type":"set_thinking_level","level":selection.thinking}))?;
    }
    let state = invoke(json!({"type":"get_state"}))?;
    let selected = actual(&state)?;
    if !models
        .iter()
        .any(|v| v["provider"] == selected.provider && v["id"] == selected.model)
        || !supported
            .iter()
            .any(|v| v.as_str() == Some(&selected.thinking))
    {
        return Err("PI_SETTINGS_UNCONFIRMED".into());
    }
    if requested.is_some_and(|r| {
        r.provider != selected.provider
            || r.model != selected.model
            || r.thinking != selected.thinking
    }) {
        return Err("PI_SETTINGS_MISMATCH".into());
    }
    if state["isStreaming"] != false || state["isCompacting"] != false {
        return Err("PI_SESSION_NOT_IDLE".into());
    }
    Ok((state, selected, catalog))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection() -> Selection {
        Selection {
            provider: "p".into(),
            model: "m".into(),
            thinking: "high".into(),
        }
    }
    fn invoke(command: &Value, thinking: &str) -> Value {
        match command["type"].as_str().unwrap() {
            "get_available_models" => json!({"models":[{"provider":"p","id":"m"}]}),
            "get_available_thinking_levels" => json!({"levels":["low","high"]}),
            "get_state" => {
                json!({"model":{"provider":"p","id":"m"},"thinkingLevel":thinking,"isStreaming":false,"isCompacting":false})
            }
            "set_model" | "set_thinking_level" => Value::Null,
            other => panic!("unexpected or persistent command {other}"),
        }
    }
    #[test]
    fn ack_cannot_hide_native_thinking_clamp() {
        let mut calls = Vec::new();
        let result = handshake_with(
            |c| {
                calls.push(c["type"].as_str().unwrap().to_owned());
                Ok(invoke(&c, "low"))
            },
            Some(&selection()),
        );
        assert_eq!(result.unwrap_err(), "PI_SETTINGS_MISMATCH");
        assert_eq!(
            calls,
            [
                "get_available_models",
                "set_model",
                "get_available_thinking_levels",
                "set_thinking_level",
                "get_state"
            ]
        );
    }
    #[test]
    fn rejects_unsupported_before_setting_thinking() {
        let mut desired = selection();
        desired.thinking = "max".into();
        let mut set = false;
        let result = handshake_with(
            |c| {
                set |= c["type"] == "set_thinking_level";
                Ok(invoke(&c, "high"))
            },
            Some(&desired),
        );
        assert_eq!(result.unwrap_err(), "PI_THINKING_NOT_AVAILABLE");
        assert!(!set);
    }
    #[test]
    fn default_is_current_native_state_without_any_setter() {
        let (_, selected, _) = handshake_with(
            |c| {
                assert!(!c["type"].as_str().unwrap().starts_with("set_"));
                Ok(invoke(&c, "low"))
            },
            None,
        )
        .unwrap();
        assert_eq!(selected.thinking, "low");
    }
}
