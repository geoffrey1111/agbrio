//! Narrow local Web Push configuration.  Raw browser capabilities and VAPID
//! signing material never enter SQLite, API responses after registration, or logs.

use base64ct::{Base64UrlUnpadded, Encoding};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use url::Url;
static CONFIG_OPERATION: Mutex<()> = Mutex::new(());
use web_push_native::{
    jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair},
    p256::PublicKey,
    Auth, WebPushBuilder,
};

const MOBILE_ALLOWED_ORIGIN_ENV: &str = "AI_WORK_ROUTER_MOBILE_ALLOWED_ORIGIN";
const PUSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PUSH_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const PUSH_FANOUT_BUDGET: Duration = Duration::from_secs(16);
const MAX_PROVIDER_ERROR_BODY_BYTES: u64 = 512;
const MAX_PROVIDER_REASON_LENGTH: usize = 96;
const MAX_APNS_REQUEST_ID_LENGTH: usize = 128;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushSubscriptionInput {
    pub endpoint: String,
    pub keys: PushSubscriptionKeys,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushSubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}
#[derive(Default, Serialize, Deserialize)]
struct LocalPushConfig {
    vapid_private_key: String,
    subscriptions: Vec<PushSubscriptionInput>,
}

/// Result of one bounded fan-out attempt.  Only a one-way fingerprint for an
/// endpoint leaves this module; raw PushSubscription material stays local.
#[derive(Debug, PartialEq, Eq)]
pub enum PushDeliveryOutcome {
    Sent {
        invalid_subscription_fingerprints: Vec<String>,
    },
    NoSubscription,
    Failed {
        invalid_subscription_fingerprints: Vec<String>,
        diagnostic: Option<PushFailureDiagnostic>,
    },
}

/// A deliberately narrow, endpoint-free provider diagnostic. It may cross the
/// authenticated mobile error boundary, unlike the subscription capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushFailureDiagnostic {
    provider: PushProviderClass,
    http_status: Option<u16>,
    provider_reason: Option<String>,
    apns_request_id: Option<String>,
    transport: Option<PushTransportClass>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PushProviderClass {
    Apple,
    Fcm,
    Mozilla,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PushTransportClass {
    Timeout,
    Network,
}

impl PushFailureDiagnostic {
    fn from_http_response(
        provider: PushProviderClass,
        status: u16,
        apple_body: Option<&[u8]>,
        apns_request_id: Option<&str>,
    ) -> Self {
        Self {
            provider,
            http_status: Some(status),
            provider_reason: (provider == PushProviderClass::Apple)
                .then(|| apple_body.and_then(bounded_apple_reason))
                .flatten(),
            apns_request_id: (provider == PushProviderClass::Apple)
                .then(|| apns_request_id.and_then(sanitized_apns_request_id))
                .flatten(),
            transport: None,
        }
    }

    fn from_transport(provider: PushProviderClass, error: &reqwest::Error) -> Self {
        Self {
            provider,
            http_status: None,
            provider_reason: None,
            apns_request_id: None,
            transport: Some(if error.is_timeout() {
                PushTransportClass::Timeout
            } else {
                PushTransportClass::Network
            }),
        }
    }

    pub fn sanitized_message(&self) -> String {
        match (
            self.provider,
            self.http_status,
            self.provider_reason.as_deref(),
            self.transport,
        ) {
            (PushProviderClass::Apple, Some(status), Some(reason), _) => {
                format!("Apple Web Push rejected the request: {reason} (provider HTTP {status})")
            }
            (provider, Some(status), _, _) => format!(
                "{} Web Push rejected the request (provider HTTP {status})",
                provider.label()
            ),
            (_, None, _, Some(PushTransportClass::Timeout)) => {
                "Web Push transport timed out before an HTTP response".into()
            }
            (_, None, _, Some(PushTransportClass::Network)) => {
                "Web Push transport failed before an HTTP response".into()
            }
            _ => "Web Push delivery failed without an HTTP response".into(),
        }
    }
}

impl PushProviderClass {
    fn label(self) -> &'static str {
        match self {
            Self::Apple => "Apple",
            Self::Fcm => "FCM",
            Self::Mozilla => "Mozilla",
            Self::Other => "Other",
        }
    }
}

fn config_path() -> Result<PathBuf, String> {
    let base =
        std::env::var("LOCALAPPDATA").map_err(|_| "Push configuration requires LOCALAPPDATA")?;
    Ok(PathBuf::from(base)
        .join("AIWorkRouter")
        .join("push")
        .join("data")
        .join("subscriptions.json"))
}
fn read_config() -> Result<LocalPushConfig, String> {
    let path = config_path()?;
    if !path.is_file() {
        return Ok(LocalPushConfig::default());
    }
    serde_json::from_str(
        &fs::read_to_string(path).map_err(|_| "Could not read local push configuration")?,
    )
    .map_err(|_| "Local push configuration is invalid".to_string())
}
fn write_config(config: &LocalPushConfig) -> Result<(), String> {
    let path = config_path()?;
    fs::create_dir_all(path.parent().ok_or("Push configuration path is invalid")?)
        .map_err(|_| "Could not create local push configuration directory")?;
    let temporary=path.with_extension("json.pending");
    fs::write(
        &temporary,
        serde_json::to_vec(config).map_err(|_| "Could not serialize local push configuration")?,
    )
    .map_err(|_| "Could not persist local push configuration".to_string())?;
    fs::rename(temporary,path).map_err(|_| "Could not finalize local push configuration".to_string())
}
fn key_pair(config: &mut LocalPushConfig) -> Result<ES256KeyPair, String> {
    if config.vapid_private_key.is_empty() {
        let key = ES256KeyPair::generate();
        config.vapid_private_key = Base64UrlUnpadded::encode_string(&key.to_bytes());
    }
    ES256KeyPair::from_bytes(
        &Base64UrlUnpadded::decode_vec(&config.vapid_private_key)
            .map_err(|_| "Local VAPID material is invalid")?,
    )
    .map_err(|_| "Local VAPID material is invalid".to_string())
}
pub fn public_key() -> Result<String, String> {
    let _guard=CONFIG_OPERATION.lock().map_err(|_| "Push configuration unavailable")?;
    let mut config = read_config()?;
    let key = key_pair(&mut config)?;
    write_config(&config)?;
    Ok(Base64UrlUnpadded::encode_string(
        &key.public_key().public_key().to_bytes_uncompressed(),
    ))
}
pub fn upsert_subscription(input: PushSubscriptionInput) -> Result<(), String> {
    validate_subscription(&input)?;
    let _guard=CONFIG_OPERATION.lock().map_err(|_| "Push configuration unavailable")?;
    let mut config = read_config()?;
    let _ = key_pair(&mut config)?;
    config
        .subscriptions
        .retain(|existing| existing.endpoint != input.endpoint);
    config.subscriptions.push(input);
    write_config(&config)
}

/// Returns only the number of locally retained browser subscriptions. Raw
/// endpoint capabilities stay in the local config and never cross an API
/// boundary.  UI must use this to distinguish a real enrolled phone from
/// stale SQLite delivery metadata.
pub fn subscription_count() -> Result<usize, String> {
    let _guard=CONFIG_OPERATION.lock().map_err(|_| "Push configuration unavailable")?;
    Ok(read_config()?.subscriptions.len())
}
pub fn subscription_fingerprint(input: &PushSubscriptionInput) -> String {
    let mut digest = Sha256::new();
    digest.update(input.endpoint.as_bytes());
    format!("{:x}", digest.finalize())
}

fn validate_subscription(input: &PushSubscriptionInput) -> Result<(), String> {
    if input.endpoint.len() > 4096 || !input.endpoint.starts_with("https://") {
        return Err("Push subscription requires a bounded HTTPS endpoint".into());
    }
    let p256dh = Base64UrlUnpadded::decode_vec(&input.keys.p256dh)
        .map_err(|_| "Push subscription public key is invalid")?;
    if input.keys.p256dh.len() > 512 || PublicKey::from_sec1_bytes(&p256dh).is_err() {
        return Err("Push subscription public key is invalid".into());
    }
    let auth = Base64UrlUnpadded::decode_vec(&input.keys.auth)
        .map_err(|_| "Push subscription auth secret is invalid")?;
    if input.keys.auth.len() > 128 || auth.len() != 16 {
        return Err("Push subscription auth secret is invalid".into());
    }
    Ok(())
}
pub fn remove_subscription(endpoint: &str) -> Result<(), String> {
    let _guard=CONFIG_OPERATION.lock().map_err(|_| "Push configuration unavailable")?;
    let mut config = read_config()?;
    config
        .subscriptions
        .retain(|existing| existing.endpoint != endpoint);
    write_config(&config)
}
/// A harmless, explicit user-requested test.  This exact plaintext is still
/// encrypted by the mature Web Push library before it leaves the Router.
pub fn send_test() -> Result<PushDeliveryOutcome, String> {
    send_payload(b"AI_WORK_ROUTER_WEB_PUSH_V1_OK")
}

/// Sends the same encrypted JSON shape used for a real terminal provider
/// observation, without claiming that a provider reply occurred.  This is a
/// user-triggered mobile diagnostic: it distinguishes a basic push-capability
/// check from the service-worker path that renders a real reply notification.
pub fn send_reply_notification_probe() -> Result<PushDeliveryOutcome, String> {
    send_payload(
        "{\"type\":\"codex_reply\",\"workstreamId\":\"\",\"observationId\":\"\",\"workstreamName\":\"通知通道自检\"}"
            .as_bytes(),
    )
}
pub fn send_payload(payload: &[u8]) -> Result<PushDeliveryOutcome, String> {
    let vapid_subject = configured_vapid_subject()?;
    let (config,key)={
        let _guard=CONFIG_OPERATION.lock().map_err(|_| "Push configuration unavailable")?;
        let mut config=read_config()?;let key=key_pair(&mut config)?;write_config(&config)?;(config,key)
    };
    if config.subscriptions.is_empty() {
        return Ok(PushDeliveryOutcome::NoSubscription);
    }
    let client = push_http_client(PUSH_CONNECT_TIMEOUT, PUSH_REQUEST_TIMEOUT)?;
    let mut succeeded = false;
    let mut invalid_endpoints = Vec::new();
    let mut first_failure = None;
    let started = Instant::now();
    for subscription in &config.subscriptions {
        let remaining = PUSH_FANOUT_BUDGET.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        let ua_public = PublicKey::from_sec1_bytes(
            &Base64UrlUnpadded::decode_vec(&subscription.keys.p256dh)
                .map_err(|_| "Stored push public key is invalid")?,
        )
        .map_err(|_| "Stored push public key is invalid")?;
        let ua_auth = Auth::clone_from_slice(
            &Base64UrlUnpadded::decode_vec(&subscription.keys.auth)
                .map_err(|_| "Stored push auth secret is invalid")?,
        );
        let provider = provider_class_for_endpoint(&subscription.endpoint);
        let request = WebPushBuilder::new(
            subscription
                .endpoint
                .parse()
                .map_err(|_| "Stored push endpoint is invalid")?,
            ua_public,
            ua_auth,
        )
        .with_vapid(&key, &vapid_subject)
        .build(payload.to_vec())
        .map_err(|_| "Could not build Web Push request")?;
        let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
            .map_err(|_| "Web Push request method is invalid")?;
        let mut outbound = client.request(method, request.uri().to_string());
        for (name, value) in request.headers() {
            outbound = outbound.header(name.as_str(), value.as_bytes());
        }
        match outbound
            .timeout(remaining.min(PUSH_REQUEST_TIMEOUT))
            .body(request.into_body())
            .send()
        {
            Ok(response) if response.status().is_success() => succeeded = true,
            Ok(mut response) => {
                let status = response.status().as_u16();
                let apns_request_id = response
                    .headers()
                    .get("apns-id")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let body = bounded_response_body(&mut response);
                first_failure.get_or_insert_with(|| {
                    PushFailureDiagnostic::from_http_response(
                        provider,
                        status,
                        body.as_deref(),
                        apns_request_id.as_deref(),
                    )
                });
                if permanently_invalid_status(status) {
                    invalid_endpoints.push(subscription.endpoint.clone());
                }
            }
            Err(error) => {
                first_failure
                    .get_or_insert_with(|| PushFailureDiagnostic::from_transport(provider, &error));
            }
        }
    }
    let invalid_subscription_fingerprints = invalid_endpoints
        .iter()
        .map(|endpoint| subscription_fingerprint_for_endpoint(endpoint))
        .collect::<Vec<_>>();
    if !invalid_endpoints.is_empty() {
        let _guard=CONFIG_OPERATION.lock().map_err(|_| "Push configuration unavailable")?;
        // Merge into current config: a concurrent enrolment must survive fan-out.
        let mut current=read_config()?;
        prune_invalid_snapshot(&mut current,&config,&invalid_endpoints);
        write_config(&current)?;
    }
    if succeeded {
        Ok(PushDeliveryOutcome::Sent {
            invalid_subscription_fingerprints,
        })
    } else {
        Ok(PushDeliveryOutcome::Failed {
            invalid_subscription_fingerprints,
            diagnostic: first_failure,
        })
    }
}

fn prune_invalid_snapshot(current:&mut LocalPushConfig,snapshot:&LocalPushConfig,invalid:&[String]) {
    current.subscriptions.retain(|subscription| !invalid.contains(&subscription.endpoint) || !snapshot.subscriptions.iter().any(|old|old.endpoint==subscription.endpoint && old.keys.p256dh==subscription.keys.p256dh && old.keys.auth==subscription.keys.auth));
}

fn configured_vapid_subject() -> Result<String, String> {
    configured_vapid_subject_from_origin(
        crate::mobile_http::configured_mobile_allowed_origin().as_deref(),
    )
}

fn configured_vapid_subject_from_origin(origin: Option<&str>) -> Result<String, String> {
    let value = origin
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!("Web Push requires {MOBILE_ALLOWED_ORIGIN_ENV} to be a configured HTTPS origin")
        })?;
    let parsed = Url::parse(value).map_err(|_| {
        format!("Web Push requires {MOBILE_ALLOWED_ORIGIN_ENV} to be a valid HTTPS origin")
    })?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || parsed.cannot_be_a_base()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(format!(
            "Web Push requires {MOBILE_ALLOWED_ORIGIN_ENV} to be a pathless HTTPS origin"
        ));
    }
    Ok(parsed.origin().ascii_serialization())
}

fn provider_class_for_endpoint(endpoint: &str) -> PushProviderClass {
    let host = Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase));
    let Some(host) = host.as_deref() else {
        return PushProviderClass::Other;
    };
    if host == "web.push.apple.com" || host.ends_with(".push.apple.com") {
        PushProviderClass::Apple
    } else if host == "fcm.googleapis.com" || host.ends_with(".fcm.googleapis.com") {
        PushProviderClass::Fcm
    } else if host == "updates.push.services.mozilla.com"
        || host.ends_with(".push.services.mozilla.com")
    {
        PushProviderClass::Mozilla
    } else {
        PushProviderClass::Other
    }
}

fn bounded_response_body(response: &mut reqwest::blocking::Response) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    response
        .take(MAX_PROVIDER_ERROR_BODY_BYTES + 1)
        .read_to_end(&mut body)
        .ok()?;
    (body.len() as u64 <= MAX_PROVIDER_ERROR_BODY_BYTES).then_some(body)
}

fn bounded_apple_reason(body: &[u8]) -> Option<String> {
    if body.len() as u64 > MAX_PROVIDER_ERROR_BODY_BYTES {
        return None;
    }
    let parsed = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    let reason = parsed.get("reason")?.as_str()?;
    (reason.len() <= MAX_PROVIDER_REASON_LENGTH
        && !reason.is_empty()
        && reason
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then(|| reason.to_owned())
}

fn sanitized_apns_request_id(value: &str) -> Option<String> {
    (value.len() <= MAX_APNS_REQUEST_ID_LENGTH
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then(|| value.to_owned())
}

fn push_http_client(
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .build()
        .map_err(|_| "Could not create Web Push HTTP client".into())
}

fn subscription_fingerprint_for_endpoint(endpoint: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(endpoint.as_bytes());
    format!("{:x}", digest.finalize())
}

fn permanently_invalid_status(status: u16) -> bool {
    matches!(status, 404 | 410)
}

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_delivery_pruning_preserves_new_and_changed_subscriptions() {
        let sub=|endpoint:&str,key:&str|super::PushSubscriptionInput{endpoint:endpoint.into(),keys:super::PushSubscriptionKeys{p256dh:key.into(),auth:"fixture".into()}};
        let snapshot=super::LocalPushConfig{vapid_private_key:String::new(),subscriptions:vec![sub("old","a"),sub("updated","old-key")]};
        let mut current=super::LocalPushConfig{vapid_private_key:String::new(),subscriptions:vec![sub("old","a"),sub("updated","new-key"),sub("new","b")]};
        super::prune_invalid_snapshot(&mut current,&snapshot,&["old".into(),"updated".into()]);
        assert_eq!(current.subscriptions.iter().map(|s|s.endpoint.as_str()).collect::<Vec<_>>(),vec!["updated","new"]);
    }
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn subscription_validation_rejects_non_https_or_malformed_capability_material() {
        let key = ES256KeyPair::generate();
        let valid = PushSubscriptionInput {
            endpoint: "https://push.example.invalid/subscription".into(),
            keys: PushSubscriptionKeys {
                p256dh: Base64UrlUnpadded::encode_string(
                    &key.public_key().public_key().to_bytes_uncompressed(),
                ),
                auth: Base64UrlUnpadded::encode_string(&[0u8; 16]),
            },
        };
        assert!(validate_subscription(&valid).is_ok());
        let mut insecure = valid.clone();
        insecure.endpoint = "http://push.example.invalid/".into();
        assert!(validate_subscription(&insecure).is_err());
        let mut malformed = valid;
        malformed.keys.auth = "not-base64".into();
        assert!(validate_subscription(&malformed).is_err());
    }

    #[test]
    fn mature_library_builds_encrypted_vapid_push_without_network() {
        let application_key = ES256KeyPair::generate();
        let user_agent_key = ES256KeyPair::generate();
        let ua_public = PublicKey::from_sec1_bytes(
            &user_agent_key
                .public_key()
                .public_key()
                .to_bytes_uncompressed(),
        )
        .unwrap();
        let request = WebPushBuilder::new(
            "https://push.example.invalid/subscription".parse().unwrap(),
            ua_public,
            Auth::clone_from_slice(&[7u8; 16]),
        )
        .with_vapid(
            &application_key,
            &configured_vapid_subject_from_origin(Some("https://router.example"))
                .expect("valid test origin"),
        )
        .build(b"AI_WORK_ROUTER_WEB_PUSH_V1_OK".to_vec())
        .unwrap();
        assert_eq!(request.method().as_str(), "POST");
        assert_eq!(request.headers()["content-encoding"], "aes128gcm");
        assert!(request.headers().contains_key("authorization"));
        assert!(!request.body().is_empty());
    }

    #[test]
    fn bounded_push_http_client_times_out_a_stalled_transport() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 512];
            let _ = socket.read(&mut request);
            thread::sleep(Duration::from_millis(200));
        });
        let started = Instant::now();
        let result = push_http_client(Duration::from_millis(20), Duration::from_millis(50))
            .unwrap()
            .get(format!("http://{address}/stalled"))
            .send();
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_millis(180));
        server.join().unwrap();
    }

    #[test]
    fn only_standard_gone_or_not_found_responses_prune_a_subscription() {
        assert!(permanently_invalid_status(404));
        assert!(permanently_invalid_status(410));
        assert!(!permanently_invalid_status(401));
        assert!(!permanently_invalid_status(429));
        assert!(!permanently_invalid_status(500));
    }

    #[test]
    fn configured_vapid_subject_uses_only_a_pathless_https_mobile_origin() {
        assert_eq!(
            configured_vapid_subject_from_origin(Some("https://router.example.invalid")).unwrap(),
            "https://router.example.invalid"
        );
        assert_eq!(
            configured_vapid_subject_from_origin(Some("https://router.example:8443/")).unwrap(),
            "https://router.example:8443"
        );
    }

    #[test]
    fn configured_vapid_subject_fails_closed_for_missing_or_non_origin_values() {
        for origin in [
            None,
            Some(""),
            Some("http://router.example"),
            Some("https://router.example/mobile"),
            Some("https://router.example/?debug=1"),
            Some("https://router.example/#fragment"),
            Some("https://user@router.example"),
        ] {
            assert!(configured_vapid_subject_from_origin(origin).is_err());
        }
    }

    #[test]
    fn apple_json_diagnostic_retains_only_bounded_reason_and_request_id() {
        let diagnostic = PushFailureDiagnostic::from_http_response(
            PushProviderClass::Apple,
            403,
            Some(br#"{"reason":"BadJwtToken"}"#),
            Some("0d5b393f-4e04-4e1f-b460-33e1b576a895"),
        );
        assert_eq!(diagnostic.provider, PushProviderClass::Apple);
        assert_eq!(diagnostic.http_status, Some(403));
        assert_eq!(diagnostic.provider_reason.as_deref(), Some("BadJwtToken"));
        assert_eq!(
            diagnostic.apns_request_id.as_deref(),
            Some("0d5b393f-4e04-4e1f-b460-33e1b576a895")
        );
        assert_eq!(
            diagnostic.sanitized_message(),
            "Apple Web Push rejected the request: BadJwtToken (provider HTTP 403)"
        );
    }

    #[test]
    fn malformed_or_oversized_apple_response_body_is_not_exposed() {
        assert_eq!(bounded_apple_reason(br#"{"reason":"Bad Jwt Token"}"#), None);
        let oversized = format!(
            r#"{{"reason":"{}"}}"#,
            "A".repeat(MAX_PROVIDER_REASON_LENGTH + 1)
        );
        assert_eq!(bounded_apple_reason(oversized.as_bytes()), None);
        let oversized_body = format!(
            r#"{{"reason":"BadJwtToken","padding":"{}"}}"#,
            "A".repeat(MAX_PROVIDER_ERROR_BODY_BYTES as usize)
        );
        assert_eq!(bounded_apple_reason(oversized_body.as_bytes()), None);
        assert_eq!(
            bounded_apple_reason(br#"{"unexpected":"BadJwtToken"}"#),
            None
        );
    }

    #[test]
    fn provider_host_classification_never_returns_an_endpoint() {
        assert_eq!(
            provider_class_for_endpoint("https://web.push.apple.com/QH/private-path"),
            PushProviderClass::Apple
        );
        assert_eq!(
            provider_class_for_endpoint("https://fcm.googleapis.com/fcm/send/private-path"),
            PushProviderClass::Fcm
        );
        assert_eq!(
            provider_class_for_endpoint("https://updates.push.services.mozilla.com/private-path"),
            PushProviderClass::Mozilla
        );
        assert_eq!(
            provider_class_for_endpoint("https://push.example.invalid/private-path"),
            PushProviderClass::Other
        );
    }

    #[test]
    fn diagnostic_message_cannot_project_subscription_capability_material() {
        let diagnostic = PushFailureDiagnostic::from_http_response(
            PushProviderClass::Apple,
            400,
            Some(br#"{"reason":"BadJwtToken"}"#),
            Some("request-1"),
        );
        let message = diagnostic.sanitized_message();
        for forbidden in [
            "web.push.apple.com/private-path",
            "p256dh",
            "auth",
            "vapid_private_key",
            "Authorization",
            "JWT",
        ] {
            assert!(!message.contains(forbidden));
        }
    }
}
