//! Official MCP webhook Events transport. OAuth/tool services remain independent.
#[path="assistant_callback_dns.rs"]
mod callback_dns;
use crate::host_application::RouterCore;
use base64ct::{Base64, Encoding};
use hmac::{Hmac, Mac};
use router_core::store::mcp_events::{
    EventFilter, EventRecord, EventSubscription, DECISION_REQUIRED, REPLY_READY,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{net::{IpAddr, SocketAddr}, time::Duration};
fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp() * 1000
}
fn iso(ms: i64) -> Result<String, String> {
    time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|_| "MCP_EVENT_TIME_INVALID")?
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| "MCP_EVENT_TIME_INVALID".into())
}
fn equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}
fn signing_key(secret: &str) -> Result<Vec<u8>, String> {
    let bytes = Base64::decode_vec(
        secret
            .strip_prefix("whsec_")
            .ok_or("MCP_EVENT_SECRET_INVALID")?,
    )
    .map_err(|_| "MCP_EVENT_SECRET_INVALID")?;
    if !(24..=64).contains(&bytes.len()) {
        return Err("MCP_EVENT_SECRET_INVALID".into());
    }
    Ok(bytes)
}
fn sign(secret: &str, id: &str, at: i64, body: &[u8]) -> Result<String, String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(&signing_key(secret)?)
        .map_err(|_| "MCP_EVENT_SECRET_INVALID")?;
    mac.update(format!("{id}.{at}.").as_bytes());
    mac.update(body);
    Ok(format!(
        "v1,{}",
        Base64::encode_string(&mac.finalize().into_bytes())
    ))
}
#[cfg(windows)]
fn protect(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| "MCP_EVENT_SECRET_STORE")?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                windows_core::PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|_| "MCP_EVENT_SECRET_STORE")?;
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}
#[cfg(not(windows))]
fn protect(_bytes: &[u8], _decrypt: bool) -> Result<Vec<u8>, String> {
    Err("MCP_EVENT_SECRET_STORE_UNSUPPORTED".into())
}
fn reveal(bytes: &[u8]) -> Result<String, String> {
    String::from_utf8(protect(bytes, true)?).map_err(|_| "MCP_EVENT_SECRET_STORE".into())
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_multicast()
                || v.is_broadcast()
                || v.is_documentation()
                || v.is_unspecified()
                || o[0] == 0
                || o[0] >= 224
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 192 && o[1] == 88 && o[2] == 99))
        }
        IpAddr::V6(v) => {
            if let Some(mapped) = v.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(mapped));
            }
            let s = v.segments();
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] <= 0x01ff || s[1] == 0x0db8))
                && s[0] != 0x2002
        }
    }
}
fn callback_url(value: &str) -> Result<url::Url, String> {
    let u = url::Url::parse(value).map_err(|_| "MCP_EVENT_CALLBACK_INVALID")?;
    if value.len() > 2048
        || u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || u.host_str().is_none()
    {
        return Err("MCP_EVENT_CALLBACK_INVALID".into());
    }
    if let Some(url::Host::Ipv4(v)) = u.host() {
        if !public_ip(IpAddr::V4(v)) {
            return Err("MCP_EVENT_CALLBACK_LITERAL_NONPUBLIC_IP".into());
        }
    }
    if let Some(url::Host::Ipv6(v)) = u.host() {
        if !public_ip(IpAddr::V6(v)) {
            return Err("MCP_EVENT_CALLBACK_LITERAL_NONPUBLIC_IP".into());
        }
    }
    Ok(u)
}
fn benchmark_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => { let o=v.octets(); o[0]==198 && matches!(o[1],18|19) },
        IpAddr::V6(v) => v.to_ipv4_mapped().is_some_and(|v| benchmark_ip(IpAddr::V4(v))),
    }
}
fn validated_dns_peers(peers: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    if peers.is_empty() { return Err("MCP_EVENT_CALLBACK_DNS_EMPTY".into()); }
    let blocked:Vec<_>=peers.iter().filter(|p| !public_ip(p.ip())).collect();
    if !blocked.is_empty() {
        // Never discard a private peer to make a mixed DNS result acceptable.
        let benchmark=blocked.iter().filter(|p| benchmark_ip(p.ip())).count();
        return Err(if benchmark==blocked.len() { "MCP_EVENT_CALLBACK_DNS_NONPUBLIC_BENCHMARK" }
            else if benchmark>0 { "MCP_EVENT_CALLBACK_DNS_NONPUBLIC_MIXED" }
            else { "MCP_EVENT_CALLBACK_DNS_NONPUBLIC_ADDRESS" }.into());
    }
    Ok(peers)
}
async fn callback_peers(u: &url::Url) -> Result<Vec<SocketAddr>, String> {
    let port = u
        .port_or_known_default()
        .ok_or("MCP_EVENT_CALLBACK_INVALID")?;
    // Literal URLs have already passed callback_url's public-address check;
    // preserve typed IPv6 rather than handing bracketed text to the resolver.
    match u.host().ok_or("MCP_EVENT_CALLBACK_INVALID")? {
        url::Host::Ipv4(v) => return Ok(vec![SocketAddr::new(IpAddr::V4(v), port)]),
        url::Host::Ipv6(v) => return Ok(vec![SocketAddr::new(IpAddr::V6(v), port)]),
        url::Host::Domain(_) => {},
    }
    let host = u.host_str().ok_or("MCP_EVENT_CALLBACK_INVALID")?;
    resolve_dns_peers(async {
        tokio::net::lookup_host((host,port)).await.map(|peers| peers.collect())
    },Duration::from_secs(3)).await
}
async fn resolve_dns_peers<F>(lookup:F,deadline:Duration)->Result<Vec<SocketAddr>,String>
where F:std::future::Future<Output=std::io::Result<Vec<SocketAddr>>> {
    let peers = tokio::time::timeout(deadline,lookup)
    .await
    .map_err(|_| "MCP_EVENT_CALLBACK_DNS_TIMEOUT")?
    .map_err(|_| "MCP_EVENT_CALLBACK_DNS_FAILED")?;
    validated_dns_peers(peers)
}
async fn safe_client(value: &str) -> Result<(reqwest::Client, url::Url), String> {
    let u = callback_url(value)?;
    let host = u.host_str().ok_or("MCP_EVENT_CALLBACK_INVALID")?;
    let peers = callback_peers_for_connect(&u).await?;
    // The client connects only to these validated peers. Preserve the hostname
    // for TLS; disable proxy environment and every redirect, including challenges.
    let client = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .resolve_to_addrs(host, &peers)
        .build()
        .map_err(|_| "MCP_EVENT_CALLBACK_CLIENT")?;
    Ok((client, u))
}
async fn callback_peers_for_connect(u:&url::Url)->Result<Vec<SocketAddr>,String> {
    match callback_peers(u).await {
        Err(e) if e=="MCP_EVENT_CALLBACK_DNS_NONPUBLIC_BENCHMARK" => callback_dns::resolve_benchmark_domain(u).await,
        other => other,
    }
}
async fn connect_callback(_gid: &str, value: &str) -> Result<(reqwest::Client, url::Url), String> {
    let u = callback_url(value)?;
    #[cfg(test)]
    if let Some(client) = TEST_CALLBACK_CLIENTS
        .lock()
        .unwrap()
        .get(&(_gid.to_string(), value.to_string()))
        .cloned()
    {
        return Ok((client, u));
    }
    safe_client(value).await
}
#[cfg(test)]
static TEST_CALLBACK_CLIENTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<(String, String), reqwest::Client>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
#[cfg(test)]
pub(crate) struct TestCallbackClient {
    key: (String, String),
}
#[cfg(test)]
impl TestCallbackClient {
    pub(crate) fn install(gid: &str, url: &str, client: reqwest::Client) -> Self {
        let key = (gid.to_string(), url.to_string());
        TEST_CALLBACK_CLIENTS
            .lock()
            .unwrap()
            .insert(key.clone(), client);
        Self { key }
    }
}
#[cfg(test)]
impl Drop for TestCallbackClient {
    fn drop(&mut self) {
        TEST_CALLBACK_CLIENTS.lock().unwrap().remove(&self.key);
    }
}

async fn post(
    core: &RouterCore,
    s: &EventSubscription,
    id: &str,
    body: Vec<u8>,
    verification: bool,
) -> Result<reqwest::Response, String> {
    if body.len() > 262144 {
        return Err("MCP_EVENT_PAYLOAD_LIMIT".into());
    }
    let (client, u) = connect_callback(&s.grant_id, &s.callback_url).await?;
    if verification {
        core.store.require_event_filter(&s.grant_id, &s.filter)?;
    } else if !core.store.event_message_still_allowed(&s.id, id)? {
        return Err("MCP_EVENT_FORBIDDEN".into());
    }
    if !verification {
        let event = core.store.read_mcp_event(&s.grant_id, id)?;
        if event.observation_id.starts_with("req_")
            && !native_request_confirmed(core, &event.observation_id)?
        {
            return Err("MCP_EVENT_SOURCE_UNCONFIRMED".into());
        }
    }
    // A key can rotate while DNS resolution is in flight. Sign with the current
    // protected keys after checking the current subscription authority.
    let current;
    let s = if verification {
        s
    } else {
        current = core
            .store
            .event_subscription(&s.id)?
            .ok_or("MCP_EVENT_FORBIDDEN")?;
        &current
    };
    let at = now() / 1000;
    let key = reveal(&s.secret)?;
    let mut signature = sign(&key, id, at, &body)?;
    if !verification && s.rotation_until.is_some_and(|until| until > now()) {
        if let Some(old) = &s.previous_secret {
            signature.push(' ');
            signature.push_str(&sign(&reveal(old)?, id, at, &body)?);
        }
    }
    client
        .post(u)
        .timeout(Duration::from_secs(if verification { 4 } else { 10 }))
        .header("content-type", "application/json")
        .header("webhook-id", id)
        .header("webhook-timestamp", at.to_string())
        .header("webhook-signature", signature)
        .header("X-MCP-Subscription-Id", &s.id)
        .body(body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "MCP_EVENT_CALLBACK_TIMEOUT"
            } else {
                "MCP_EVENT_CALLBACK_NETWORK"
            }
            .into()
        })
}
pub(crate) fn native_request_confirmed(core: &RouterCore, obs: &str) -> Result<bool, String> {
    let Some((thread, turn, raw)) = core.store.mcp_request_identity(obs)? else {
        return Ok(false);
    };
    let session = core
        .session
        .lock()
        .map_err(|_| "MCP_EVENT_SOURCE_UNCONFIRMED")?;
    Ok(session.pending_codex_requests.values().any(|r| {
        r.thread_id == thread
            && r.turn_id == turn
            && r.raw_request_id.to_string() == raw
            && !r.responded
    }))
}
fn subscription_id(gid: &str, name: &str, args: &EventFilter, url: &str) -> Result<String, String> {
    let arguments = serde_json::to_value(args).map_err(|_| "MCP_EVENT_ARGUMENTS_INVALID")?;
    let identity = json!([gid, url, name, arguments]);
    Ok(format!(
        "sub_{}",
        hex(&serde_json::to_vec(&identity).map_err(|_| "MCP_EVENT_ARGUMENTS_INVALID")?)
    ))
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn catalog() -> Value {
    let filters = json!({"type":"object","properties":{"workstreamId":{"type":"string","minLength":1},"bindingRevision":{"type":"integer","minimum":0},"sourceRole":{"type":"string","enum":["DECISION","EXECUTION"]}},"required":["workstreamId","bindingRevision","sourceRole"],"additionalProperties":false});
    let payload = json!({"type":"object","properties":{"workstreamId":{"type":"string"},"bindingRevision":{"type":"integer"},"observationId":{"type":"string"},"sourceRole":{"type":"string","enum":["DECISION","EXECUTION"]},"sourceKind":{"type":"string","enum":["REPLY","NATIVE_REQUEST"]},"rootEventId":{"type":"string"},"hopCount":{"type":"integer"},"reason":{"type":["string","null"]}},"required":["workstreamId","bindingRevision","observationId","sourceRole","sourceKind","rootEventId","hopCount","reason"],"additionalProperties":false});
    json!({"events":[{"name":REPLY_READY,"description":"A complete new reply is ready on the exact authorized Bridge source. Metadata only; retrieve the exact full source with existing read tools.","delivery":["webhook"],"inputSchema":filters,"payloadSchema":payload},{"name":DECISION_REQUIRED,"description":"An exact Bridge result requires an owner decision or reached the bounded relay-cycle/uncertain-receipt boundary.","delivery":["webhook"],"inputSchema":filters,"payloadSchema":payload}]})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Delivery {
    mode: String,
    url: String,
    secret: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Subscribe {
    name: String,
    arguments: EventFilter,
    delivery: Delivery,
    cursor: Option<String>,
    ttl_ms: Option<u64>,
}
async fn subscribe(
    core: &RouterCore,
    gid: &str,
    input: Value,
    remove: bool,
) -> Result<Value, String> {
    let args: Subscribe =
        serde_json::from_value(input.clone()).map_err(|_| "MCP_EVENT_ARGUMENTS_INVALID")?;
    if !matches!(args.name.as_str(), REPLY_READY | DECISION_REQUIRED)
        || args.delivery.mode != "webhook"
        || args.cursor.is_some()
    {
        return Err("MCP_EVENT_ARGUMENTS_INVALID".into());
    }
    callback_url(&args.delivery.url)?;
    let sid = subscription_id(gid, &args.name, &args.arguments, &args.delivery.url)?;
    if remove {
        core.store.unsubscribe_event(gid, &sid)?;
        return Ok(json!({}));
    }
    core.store.require_event_filter(gid, &args.arguments)?;
    let secret = args.delivery.secret.ok_or("MCP_EVENT_SECRET_INVALID")?;
    signing_key(&secret)?;
    let at = now();
    let grant = core.store.assistant_grant(gid)?;
    let ttl = args.ttl_ms.unwrap_or(86400000);
    if ttl == 0 {
        return Err("MCP_EVENT_ARGUMENTS_INVALID".into());
    }
    let expiry = (at + ttl.min(86400000) as i64).min(grant.expires_at);
    let old = core.store.event_subscription(&sid)?;
    let same = old
        .as_ref()
        .and_then(|s| reveal(&s.secret).ok())
        .is_some_and(|key| equal(key.as_bytes(), secret.as_bytes()));
    let mut sub = EventSubscription {
        id: sid,
        grant_id: gid.into(),
        name: args.name,
        filter: args.arguments,
        callback_url: args.delivery.url,
        secret: protect(secret.as_bytes(), false)?,
        previous_secret: None,
        rotation_until: None,
        verified_at: at,
        expires_at: expiry,
        status: "ACTIVE".into(),
    };
    if let Some(old) = old
        .as_ref()
        .filter(|s| s.status == "ACTIVE" && s.expires_at > at && !s.secret.is_empty())
    {
        if same {
            sub.previous_secret = old.previous_secret.clone();
            sub.rotation_until = old.rotation_until;
        } else {
            sub.previous_secret = Some(old.secret.clone());
            sub.rotation_until = Some(at + 300000);
        }
    }
    let cached = core
        .store
        .recently_verified_event_callbacks(gid, &sub.callback_url)?
        .into_iter()
        .find(|s| {
            reveal(&s.secret)
                .ok()
                .is_some_and(|key| equal(key.as_bytes(), secret.as_bytes()))
        });
    if cached.is_none() {
        let challenge = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let message = format!("msg_verification_{}", uuid::Uuid::new_v4().simple());
        let body = serde_json::to_vec(&json!({"type":"verification","challenge":challenge}))
            .map_err(|_| "MCP_EVENT_ARGUMENTS_INVALID")?;
        let mut response = post(core, &sub, &message, body, true).await?;
        if !response.status().is_success() {
            return Err("MCP_EVENT_CALLBACK_CHALLENGE_FAILED".into());
        }
        let echoed = tokio::time::timeout(Duration::from_secs(4), async {
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "MCP_EVENT_CALLBACK_CHALLENGE_FAILED")?
            {
                if bytes.len() + chunk.len() > 8192 {
                    return Err("MCP_EVENT_CALLBACK_CHALLENGE_FAILED");
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| "MCP_EVENT_CALLBACK_CHALLENGE_FAILED")
        })
        .await
        .map_err(|_| "MCP_EVENT_CALLBACK_TIMEOUT")??;
        if !echoed["challenge"]
            .as_str()
            .is_some_and(|v| equal(v.as_bytes(), challenge.as_bytes()))
        {
            return Err("MCP_EVENT_CALLBACK_CHALLENGE_FAILED".into());
        }
    } else if let Some(verified) = cached {
        sub.verified_at = verified.verified_at;
    }
    core.store.save_event_subscription(&sub)?;
    Ok(json!({"id":sub.id,"refreshBefore":iso(expiry)?,"cursor":null,"truncated":false}))
}
pub(crate) async fn rpc(core: &RouterCore, gid: &str, input: Value) -> Value {
    let id = input.get("id").cloned().unwrap_or(Value::Null);
    if !input.is_object()
        || input["jsonrpc"] != "2.0"
        || input
            .get("id")
            .is_none_or(|i| !i.is_string() && !i.is_number())
    {
        return json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":"Invalid Request"}});
    }
    let grant = match core.store.assistant_grant(gid) {
        Ok(g) => g,
        Err(_) => {
            return json!({"jsonrpc":"2.0","id":id,"error":{"code":-32024,"message":"MCP_EVENT_FORBIDDEN"}})
        }
    };
    let method = input["method"].as_str().unwrap_or("");
    let mut parameters = input.get("params").cloned().unwrap_or(json!({}));
    // MCP request metadata is a transport field, not subscription arguments.
    // Never interpret it as authorization, filter overrides or instructions.
    if let Some(fields)=parameters.as_object_mut() {
        if fields.get("_meta").is_some_and(|v| !v.is_object()&&!v.is_null()) {
            return json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"MCP_EVENT_REQUEST_META_INVALID"}});
        }
        fields.remove("_meta");
    }
    let result = match method {
        "events/list" => {
            if parameters.as_object().is_none_or(|v| {
                v.keys().any(|k| k != "cursor") || v.get("cursor").is_some_and(|c| !c.is_null())
            }) {
                Err("MCP_EVENT_ARGUMENTS_INVALID".into())
            } else {
                let mut result = catalog();
                for definition in result["events"].as_array_mut().unwrap() {
                    if grant.source_role != "BOTH" {
                        definition["inputSchema"]["properties"]["sourceRole"]["enum"] =
                            json!([grant.source_role]);
                    }
                    if grant.scope != "INSTANCE" {
                        definition["inputSchema"]["properties"]["workstreamId"]["const"] =
                            json!(grant.workstream_id);
                        definition["inputSchema"]["properties"]["bindingRevision"]["const"] =
                            json!(grant.binding_revision);
                    }
                }
                Ok(result)
            }
        }
        "events/subscribe" => subscribe(core, gid, parameters, false).await,
        "events/unsubscribe" => subscribe(core, gid, parameters, true).await,
        _ => Err("MCP_EVENT_METHOD_UNSUPPORTED".into()),
    };
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(e) => {
            let (code, reason) = if e == "MCP_EVENT_METHOD_UNSUPPORTED" {
                (-32601, "method_not_found")
            } else if e.starts_with("MCP_EVENT_CALLBACK") {
                (
                    -32015,
                    if e.contains("TIMEOUT") {
                        "timeout"
                    } else if e.contains("CHALLENGE") {
                        "challenge_failed"
                    } else {
                        "endpoint_unreachable"
                    },
                )
            } else if e.contains("FORBIDDEN") || e.contains("REVOKED") || e.contains("CHANGED") {
                (-32024, "forbidden")
            } else if e.contains("LIMIT") {
                (-32013, "subscriptions")
            } else {
                (-32602, "invalid_params")
            };
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":e,"data":{"reason":reason}}})
        }
    }
}
pub(crate) async fn dispatch_one(core: &RouterCore) {
    let Ok(Some(delivery)) = core.store.claim_event_delivery() else {
        return;
    };
    let s = &delivery.subscription;
    let e = &delivery.event;
    if !core
        .store
        .event_message_still_allowed(&s.id, &e.event_id)
        .unwrap_or(false)
    {
        return;
    }
    let value = match envelope(e) {
        Ok(v) => v,
        Err(_) => {
            let _ =
                core.store
                    .finish_event_delivery(&s.id, &e.event_id, delivery.attempts, Some(413));
            return;
        }
    };
    let response = post(
        core,
        s,
        &e.event_id,
        serde_json::to_vec(&value).unwrap_or_default(),
        false,
    )
    .await;
    let status = response.ok().map(|r| r.status().as_u16());
    let _ = core
        .store
        .finish_event_delivery(&s.id, &e.event_id, delivery.attempts, status);
}
fn envelope(e: &EventRecord) -> Result<Value, String> {
    Ok(
        json!({"eventId":e.event_id,"name":e.name,"timestamp":iso(e.occurred_at)?,"data":{"workstreamId":e.workstream_id,"bindingRevision":e.binding_revision,"observationId":e.observation_id,"sourceRole":e.source_role,"sourceKind":if e.observation_id.starts_with("req_"){"NATIVE_REQUEST"}else{"REPLY"},"rootEventId":e.root_event_id,"hopCount":e.hop_count,"reason":e.reason},"cursor":null}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_address_categories_preserve_mixed_peer_denial_and_literal_guards() {
        let peers=|ips:&[&str]| ips.iter().map(|ip| SocketAddr::new(ip.parse().unwrap(),443)).collect::<Vec<_>>();
        for input in ["https://10.0.0.1/private?token=do-not-log", "https://[::ffff:198.18.1.1]/secret"] {
            assert_eq!(callback_url(input).unwrap_err(),"MCP_EVENT_CALLBACK_LITERAL_NONPUBLIC_IP");
        }
        assert_eq!(validated_dns_peers(vec![]).unwrap_err(),"MCP_EVENT_CALLBACK_DNS_EMPTY");
        assert_eq!(validated_dns_peers(peers(&["1.1.1.1","10.0.0.1"])).unwrap_err(),"MCP_EVENT_CALLBACK_DNS_NONPUBLIC_ADDRESS");
        assert_eq!(validated_dns_peers(peers(&["198.18.1.1","1.1.1.1"])).unwrap_err(),"MCP_EVENT_CALLBACK_DNS_NONPUBLIC_BENCHMARK");
        assert_eq!(validated_dns_peers(peers(&["::ffff:198.19.1.1"])).unwrap_err(),"MCP_EVENT_CALLBACK_DNS_NONPUBLIC_BENCHMARK");
        assert_eq!(validated_dns_peers(peers(&["198.18.1.1","10.0.0.1","1.1.1.1"])).unwrap_err(),"MCP_EVENT_CALLBACK_DNS_NONPUBLIC_MIXED");
        assert_eq!(validated_dns_peers(peers(&["1.1.1.1","2606:4700:4700::1111"])).unwrap().len(),2);
        let runtime=tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let u=callback_url("https://[2606:4700:4700::1111]:8443/check").unwrap();
            assert_eq!(callback_peers(&u).await.unwrap(),peers(&["2606:4700:4700::1111"]).into_iter().map(|mut p| {p.set_port(8443);p}).collect::<Vec<_>>());
            assert_eq!(resolve_dns_peers(async {Ok(vec![])},Duration::from_secs(1)).await.unwrap_err(),"MCP_EVENT_CALLBACK_DNS_EMPTY");
            assert_eq!(resolve_dns_peers(async {Err(std::io::Error::other("private-url?key=private-secret"))},Duration::from_secs(1)).await.unwrap_err(),"MCP_EVENT_CALLBACK_DNS_FAILED");
            assert_eq!(resolve_dns_peers(std::future::pending(),Duration::from_millis(1)).await.unwrap_err(),"MCP_EVENT_CALLBACK_DNS_TIMEOUT");
        });
    }
    #[test]
    #[ignore="Explicit read-only system DNS of two fixed public hosts; no HTTP, subscriptions or inference"]
    fn actual_windows_system_callback_dns_classification_without_http() {
        let runtime=tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            for (index,host) in ["chatgpt.com","api.openai.com"].into_iter().enumerate() {
                let u=callback_url(&format!("https://{host}/")).unwrap();
                let result=callback_peers(&u).await;
                let code=match result {Ok(_)=>"PUBLIC_PEERS_VALIDATED".to_string(),Err(e)=>e};
                // Fixed labels only, no hostname, raw IP, URL or native errors.
                println!("{}",json!({"probe":index+1,"resolverPath":"TOKIO_OS_SYSTEM_GETADDRINFO","result":code,"httpRequested":false}));
            }
        });
    }
    #[test]
    fn blocks_non_public_callbacks_and_mapped_addresses() {
        for u in [
            "http://example.com/x",
            "https://127.0.0.1/x",
            "https://10.0.0.1/x",
            "https://[::1]/x",
            "https://[::ffff:127.0.0.1]/x",
            "https://169.254.169.254/x",
            "https://100.64.0.1/x",
            "https://192.0.2.1/x",
            "https://[2001:db8::1]/x",
            "https://user:pass@example.com/x",
        ] {
            assert!(callback_url(u).is_err(), "{u}");
        }
        assert!(callback_url("https://example.com/mcp/callback").is_ok());
    }
    #[test]
    fn signature_matches_standard_webhooks_exact_raw_body_vector() {
        let secret = format!("whsec_{}", Base64::encode_string(&[1u8; 32]));
        let body = b"{\"eventId\":\"evt_test\"}";
        let a = sign(&secret, "evt_test", 1700000000, body).unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(&[1u8; 32]).unwrap();
        mac.update(b"evt_test.1700000000.{\"eventId\":\"evt_test\"}");
        assert_eq!(
            a,
            format!("v1,{}", Base64::encode_string(&mac.finalize().into_bytes()))
        );
        assert_ne!(
            a,
            sign(
                &secret,
                "evt_test",
                1700000000,
                b"{ \"eventId\":\"evt_test\"}"
            )
            .unwrap()
        );
        assert!(sign("whsec_abc", "x", 0, b"{}").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn new_webhook_keys_are_owner_protected_and_corruption_fails() {
        let fake = b"fixture-only-webhook-key";
        let encrypted = protect(fake, false).unwrap();
        assert_ne!(encrypted, fake);
        assert_eq!(protect(&encrypted, true).unwrap(), fake);
        assert!(protect(b"not-dpapi", true).is_err());
    }
}
