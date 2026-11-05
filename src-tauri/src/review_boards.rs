//! Explicit owner-registered trusted review application. Not arbitrary HTML/URL proxy.
//! Original service owns data, decisions, CSRF, source material identity and receipts.
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
const MAX_TEXT: usize = 32 * 1024 * 1024;
const MAX_MEDIA: u64 = 512 * 1024 * 1024;
const RANGE_BYTES: u64 = 4 * 1024 * 1024;
const SCRIPTS: &[&str] = &[
    "board-loader.js",
    "app.js",
    "production-ui.js",
    "batch-selection.js",
    "review-backup.js",
    "approval-ui.js",
    "illustration-ui.js",
    "motion-review-ui.js",
];
static CHANGE: Mutex<()> = Mutex::new(());
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Registration {
    pub id: String,
    pub root: PathBuf,
    pub title: String,
    pub port: u16,
    pub base_path: String,
    pub legacy_base: Option<String>,
}
fn store_path() -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        return TEST_STORE
            .lock()
            .map_err(|_| "REVIEW_BOARD_BUSY")?
            .clone()
            .ok_or("REVIEW_BOARD_TEST_STORE_UNSET".into());
    }
    #[allow(unreachable_code)]
    router_core::platform::local_data_base().ok()
        .map(|p| PathBuf::from(p).join("AIWorkRouter/data/review-boards.json"))
        .ok_or("REVIEW_BOARD_STORE_UNAVAILABLE".into())
}
#[cfg(test)]
static TEST_STORE: Mutex<Option<PathBuf>> = Mutex::new(None);
fn rows_at(path: &Path) -> Result<Vec<Registration>, String> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let b = fs::read(path).map_err(|_| "REVIEW_BOARD_STORE_UNAVAILABLE")?;
    if b.len() > 64 * 1024 {
        return Err("REVIEW_BOARD_STORE_INVALID".into());
    }
    serde_json::from_slice(&b).map_err(|_| "REVIEW_BOARD_STORE_INVALID".into())
}
fn save(path: &Path, rows: &[Registration]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "REVIEW_BOARD_STORE_UNAVAILABLE")?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec(rows).map_err(|_| "REVIEW_BOARD_STORE_INVALID")?,
    )
    .map_err(|_| "REVIEW_BOARD_STORE_UNAVAILABLE")?;
    fs::rename(tmp, path).map_err(|_| "REVIEW_BOARD_STORE_UNAVAILABLE".into())
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub(crate) fn registration(id: &str) -> Result<Registration, String> {
    if !valid_id(id) {
        return Err("REVIEW_BOARD_ID_INVALID".into());
    }
    rows_at(&store_path()?)?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("REVIEW_BOARD_NOT_FOUND".into())
}
pub(crate) fn remove(id: &str) -> Result<(), String> {
    let _guard = CHANGE.lock().map_err(|_| "REVIEW_BOARD_BUSY")?;
    let path = store_path()?;
    let mut rows = rows_at(&path)?;
    if !rows.iter().any(|r| r.id == id) {
        return Err("REVIEW_BOARD_NOT_FOUND".into());
    }
    rows.retain(|r| r.id != id);
    save(&path, &rows)
}
fn source_url(r: &Registration, rest: &str) -> String {
    format!("http://127.0.0.1:{}{}/{}", r.port, r.base_path, rest)
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "REVIEW_BOARD_SOURCE_UNAVAILABLE".into())
}
async fn limited(response: reqwest::Response, max: usize) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > max as u64) {
        return Err("REVIEW_BOARD_RESPONSE_TOO_LARGE".into());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "REVIEW_BOARD_SOURCE_UNAVAILABLE")?;
        if bytes.len() + chunk.len() > max {
            return Err("REVIEW_BOARD_RESPONSE_TOO_LARGE".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn health(r: &Registration) -> Result<Value, String> {
    let response = client()?
        .get(source_url(r, "api/health"))
        .send()
        .await
        .map_err(|_| "REVIEW_BOARD_SOURCE_UNAVAILABLE")?;
    if !response.status().is_success() {
        return Err("REVIEW_BOARD_SOURCE_UNAVAILABLE".into());
    }
    let v: Value = serde_json::from_slice(&limited(response, 16 * 1024).await?)
        .map_err(|_| "REVIEW_BOARD_IDENTITY_CHANGED")?;
    let root = PathBuf::from(v["root"].as_str().ok_or("REVIEW_BOARD_IDENTITY_CHANGED")?)
        .canonicalize()
        .map_err(|_| "REVIEW_BOARD_IDENTITY_CHANGED")?;
    if v["service"] != "card-character-review-board"
        || root != r.root
        || v["basePath"] != r.base_path
        || v["port"].as_u64() != Some(r.port as u64)
    {
        return Err("REVIEW_BOARD_IDENTITY_CHANGED".into());
    }
    Ok(v)
}
pub(crate) fn publish(
    folder: &str,
    port: u16,
    title: &str,
) -> Result<crate::published_boards::BoardInfo, String> {
    let folder = folder.to_owned();
    let title = title.to_owned();
    std::thread::spawn(move || {
        let _guard = CHANGE.lock().map_err(|_| "REVIEW_BOARD_BUSY")?;
        let input = PathBuf::from(&folder);
        if !input.is_absolute()
            || folder.starts_with("\\\\")
            || folder.starts_with("//")
            || port < 1024
            || title.trim().is_empty()
            || title.chars().count() > 80
            || title.chars().any(char::is_control)
        {
            return Err("REVIEW_BOARD_CONFIG_INVALID".into());
        }
        crate::character_board::no_reparse(&input)?;
        let root = input
            .canonicalize()
            .map_err(|_| "REVIEW_BOARD_ROOT_UNAVAILABLE")?;
        for file in ["server.mjs", "reviews.mjs", "index.html"] {
            crate::character_board::no_reparse(&root.join(file))?;
            if !root.join(file).is_file() {
                return Err("REVIEW_BOARD_ROOT_UNAVAILABLE".into());
            }
        }
        let runtime =
            tokio::runtime::Runtime::new().map_err(|_| "REVIEW_BOARD_SOURCE_UNAVAILABLE")?;
        let path = store_path()?;
        let mut rows = rows_at(&path)?;
        let previous = rows.iter().find(|r| r.root == root).cloned();
        let mut row = Registration {
            id: previous
                .as_ref()
                .map(|r| r.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
            root,
            title: title.trim().into(),
            port,
            base_path: "/board".into(),
            legacy_base: None,
        };
        let h = match runtime.block_on(health(&row)) {
            Ok(v) => v,
            Err(_) => {
                row.base_path.clear();
                runtime.block_on(health(&row))?
            }
        };
        if let Some(crate::character_board::Source::Url { url, .. }) =
            crate::character_board::source()
        {
            if let Ok(old) = url::Url::parse(&url) {
                let expected = format!(
                    "{}{}",
                    h["publicOrigin"].as_str().unwrap_or_default(),
                    row.base_path
                );
                if old.as_str().trim_end_matches('/') == expected {
                    row.legacy_base = Some(old.to_string());
                }
            }
        }
        if let Some(old) = rows.iter_mut().find(|r| r.id == row.id) {
            *old = row.clone()
        } else {
            if rows.len() + crate::published_boards::static_list()?.len() >= 32 {
                return Err("PUBLISHED_BOARD_LIMIT".into());
            }
            rows.push(row.clone())
        }
        save(&path, &rows)?;
        Ok(info(&row))
    })
    .join()
    .map_err(|_| "REVIEW_BOARD_PUBLISH_UNCONFIRMED")?
}
fn info(r: &Registration) -> crate::published_boards::BoardInfo {
    crate::published_boards::BoardInfo {
        id: r.id.clone(),
        title: r.title.clone(),
        revision: String::new(),
        available: r.root.is_dir(),
        kind: Some("REVIEW".into()),
        replaces_legacy: r.legacy_base.is_some(),
    }
}
pub(crate) fn list() -> Result<Vec<crate::published_boards::BoardInfo>, String> {
    let path = match store_path() {
        Ok(p) => p,
        Err(e) if e == "REVIEW_BOARD_TEST_STORE_UNSET" => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    Ok(rows_at(&path)?.iter().map(info).collect())
}
pub(crate) async fn desktop_url(id: String) -> Result<String, String> {
    let r = registration(&id)?;
    health(&r).await?;
    Ok(source_url(&r, ""))
}
fn pairs(query: Option<&str>) -> Vec<(String, String)> {
    url::form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .into_owned()
        .collect()
}
fn read_allowed(path: &str, query: Option<&str>) -> bool {
    let q = pairs(query);
    if path.contains(['\\', '\0'])
        || path.split('/').any(|p| matches!(p, "." | ".."))
        || q.iter()
            .enumerate()
            .any(|(i, (k, _))| q[..i].iter().any(|(old, _)| old == k))
    {
        return false;
    }
    if path == "api/works" {
        return q.is_empty();
    }
    if path == "api/reviews" {
        return q.is_empty()
            || q.len() == 1
                && q[0].0 == "scope"
                && matches!(
                    q[0].1.as_str(),
                    "illustrations" | "ui-resources" | "motions" | "motion-review"
                );
    }
    if path.starts_with("assets/") {
        let id = &path[7..];
        return id.len() == 64
            && id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            && q.iter().all(|(k, v)| match k.as_str() {
                "s" => v.len() == 20 && v.bytes().all(|b| b.is_ascii_hexdigit()),
                "v" => (16..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_hexdigit()),
                "preview" => matches!(v.as_str(), "240" | "540" | "900"),
                "pv" => v == "1",
                _ => false,
            });
    }
    if path.starts_with("data/chunks/") {
        let name = &path[12..];
        let p: Vec<_> = name.split('.').collect();
        return q.is_empty()
            && p.len() == 3
            && p[0].len() <= 128
            && p[0]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            && p[1].len() == 20
            && p[1].bytes().all(|b| b.is_ascii_hexdigit())
            && matches!(p[2], "json" | "html");
    }
    if path.is_empty() || path == "index.html" {
        return q.iter().all(|(k, v)| match k.as_str() {
            "_agbrio_refresh" => v.len() <= 20 && v.bytes().all(|b| b.is_ascii_digit()),
            "group" => {
                !v.is_empty()
                    && v.len() <= 60
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            }
            _ => false,
        });
    }
    let static_file = SCRIPTS.contains(&path)
        || matches!(
            path,
            "style.css"
                | "production.css"
                | "illustration.css"
                | "motion-review.css"
                | "data/board-data.js"
                | "favicon.ico"
                | "技能动作需求初判_20261028.md"
                | "未知进度格_20261028.md"
                | "美术生产总看板_交付说明_20261028.md"
                | "维护说明.md"
        );
    static_file
        && q.iter()
            .all(|(k, v)| k == "v" && v.len() == 16 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn bounded_range(value: &str) -> Option<String> {
    let raw = value.strip_prefix("bytes=")?;
    let (a, b) = raw.split_once('-')?;
    if a.is_empty() {
        let suffix = b.parse::<u64>().ok()?;
        return (suffix > 0).then(|| format!("bytes=-{}", suffix.min(RANGE_BYTES)));
    }
    let start = a.parse::<u64>().ok()?;
    let end = if b.is_empty() {
        start.saturating_add(RANGE_BYTES - 1)
    } else {
        let end = b.parse::<u64>().ok()?;
        if end < start {
            return None;
        }
        end.min(start.saturating_add(RANGE_BYTES - 1))
    };
    Some(format!("bytes={start}-{end}"))
}
fn write_allowed(path: &str, query: Option<&str>) -> bool {
    query.unwrap_or_default().is_empty()
        && (matches!(path, "api/draft" | "api/submit")
            || path
                .strip_prefix("api/notify/")
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id)))
}
fn mount(id: &str) -> String {
    format!("/v1/mobile/review-boards/{id}/frame/")
}
fn mapped_url(value: &str, r: &Registration, public_origin: &str) -> String {
    let source = format!("{}/", r.base_path);
    if value.starts_with(&source) {
        return format!("{}{}", mount(&r.id), &value[source.len()..]);
    }
    if let Some(old) = r.legacy_base.as_ref() {
        if value.starts_with(old) {
            return format!("{}{}", mount(&r.id), &value[old.len()..]);
        }
    }
    let _ = public_origin;
    value.into()
}
fn maps(v: &mut Value, r: &Registration, origin: &str) {
    for name in ["assetURLs", "manifest", "scripts"] {
        if let Some(map) = v.get_mut(name).and_then(Value::as_object_mut) {
            for value in map.values_mut() {
                if let Some(s) = value.as_str() {
                    *value = Value::String(mapped_url(s, r, origin));
                }
            }
        }
    }
    if let Some(v) = v.get_mut("bootstrap") {
        maps(v, r, origin)
    }
}
fn frame_document(
    text: &str,
    r: &Registration,
    origin: &str,
    nonce: &str,
) -> Result<String, String> {
    let doc = dom_query::Document::from(text);
    let bootstrap = doc.select("#board-bootstrap");
    if bootstrap.length() != 1 {
        return Err("REVIEW_BOARD_ENTRY_INVALID".into());
    }
    let mut data: Value =
        serde_json::from_str(&bootstrap.text()).map_err(|_| "REVIEW_BOARD_ENTRY_INVALID")?;
    maps(&mut data, r, origin);
    bootstrap.set_text(
        &serde_json::to_string(&data)
            .map_err(|_| "REVIEW_BOARD_ENTRY_INVALID")?
            .replace('<', "\\u003c"),
    );
    doc.select("base, meta[http-equiv]").remove();
    for node in doc.select("[src],[href],[poster],[data-wall-src]").nodes() {
        for attr in ["src", "href", "poster", "data-wall-src"] {
            if let Some(old) = node.attr(attr) {
                node.set_attr(attr, &mapped_url(&old, r, origin));
            }
        }
    }
    // Only the original approved loader executes inline. Published data does not
    // acquire script authority by being inserted into this document.
    let loader = fs::read_to_string(r.root.join("board-loader.js"))
        .map_err(|_| "REVIEW_BOARD_ENTRY_INVALID")?;
    for node in doc.select("script").nodes() {
        let ty = node.attr("type").map(|v| v.to_string()).unwrap_or_default();
        if ty == "application/json" {
            continue;
        }
        if let Some(src) = node.attr("src") {
            let path = url::Url::parse(&format!("{origin}{}", mount(&r.id)))
                .ok()
                .and_then(|base| base.join(&src).ok())
                .map(|u| u.path().rsplit('/').next().unwrap_or_default().to_string())
                .unwrap_or_default();
            if !SCRIPTS.contains(&path.as_str()) {
                return Err("REVIEW_BOARD_SCRIPT_REFUSED".into());
            }
        } else if node.text().trim() != loader.trim()
            && node.text().trim() != loader.replace("</script", "<\\/script").trim()
        {
            return Err("REVIEW_BOARD_INLINE_SCRIPT_REFUSED".into());
        }
        node.set_attr("nonce", nonce);
    }
    let base = format!("{}{}", origin, mount(&r.id));
    let config = json!({"mount":mount(&r.id),"sourcePrefix":format!("{}/",r.base_path),"legacyBase":r.legacy_base.as_ref().filter(|b|b.starts_with(&(origin.to_string()+"/"))),"base":base});
    let code = include_str!("../../src/features/character-board/review-service-context.js")
        .replace(
            "__AGBRIO_REVIEW_CONFIG__",
            &config.to_string().replace('<', "\\u003c"),
        );
    doc.select("head").prepend_html(format!(
        "<base href=\"{base}\"><script nonce=\"{nonce}\">{}</script>",
        code.replace("</script", "<\\/script")
    ));
    Ok(format!("<!doctype html>{}", doc.html()))
}
fn fail(status: StatusCode, code: &str) -> Response {
    Response::builder().status(status).header("content-type","application/json").header("cache-control","no-store").body(Body::from(json!({"error":code,"delivery":if status==StatusCode::GATEWAY_TIMEOUT{"UNKNOWN"}else{"NOT_ATTEMPTED"}}).to_string())).unwrap()
}
/// Caller must establish DEVICE owner authentication (never the MCP/Access fallback).
pub(crate) async fn proxy(
    id: String,
    path: String,
    query: Option<String>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
    origin: String,
) -> Response {
    let r = match registration(&id) {
        Ok(r) => r,
        Err(_) => return fail(StatusCode::NOT_FOUND, "REVIEW_BOARD_NOT_FOUND"),
    };
    let write = method == Method::POST;
    if !(method == Method::GET && read_allowed(&path, query.as_deref())
        || write && write_allowed(&path, query.as_deref()))
    {
        return fail(StatusCode::FORBIDDEN, "REVIEW_BOARD_ROUTE_BLOCKED");
    }
    if write
        && (body.len() > 64 * 1024
            || headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_none_or(|v| !v.starts_with("application/json")))
    {
        return fail(StatusCode::PAYLOAD_TOO_LARGE, "REVIEW_BOARD_BODY_LIMIT");
    }
    if health(&r).await.is_err() {
        return fail(
            StatusCode::SERVICE_UNAVAILABLE,
            "REVIEW_BOARD_SOURCE_UNAVAILABLE",
        );
    }
    let client = match client() {
        Ok(v) => v,
        Err(_) => {
            return fail(
                StatusCode::SERVICE_UNAVAILABLE,
                "REVIEW_BOARD_SOURCE_UNAVAILABLE",
            )
        }
    };
    let mut url = match url::Url::parse(&source_url(&r, "")) {
        Ok(v) => v,
        Err(_) => return fail(StatusCode::BAD_GATEWAY, "REVIEW_BOARD_SOURCE_INVALID"),
    };
    url.set_path(&format!("{}/{}", r.base_path, path));
    url.set_query(query.as_deref());
    let mut request = client
        .request(method.clone(), url)
        .header("accept-encoding", "identity");
    if write {
        let token = headers
            .get("x-board-token")
            .and_then(|s| s.to_str().ok())
            .unwrap_or("");
        if token.len() != 48 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return fail(StatusCode::FORBIDDEN, "REVIEW_BOARD_TOKEN_REQUIRED");
        }
        request = request
            .header("origin", format!("http://127.0.0.1:{}", r.port))
            .header("content-type", "application/json")
            .header("x-board-token", token)
            .body(body);
    }
    if !write {
        if let Some(v) = headers.get("if-none-match") {
            if !path.ends_with(".json") && path != "api/works" {
                request = request.header("if-none-match", v);
            }
        }
        if let Some(v) = headers.get("range").and_then(|v| v.to_str().ok()) {
            let Some(range) = bounded_range(v) else {
                return fail(
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    "REVIEW_BOARD_RANGE_INVALID",
                );
            };
            request = request.header("range", range);
        }
    }

    let response = match request.send().await {
        Ok(v) => v,
        Err(_) => {
            return fail(
                if write {
                    StatusCode::GATEWAY_TIMEOUT
                } else {
                    StatusCode::BAD_GATEWAY
                },
                "REVIEW_BOARD_RESPONSE_UNCONFIRMED",
            )
        }
    };
    let status = response.status();
    let upstream = response.headers().clone();
    let mime = upstream
        .get("content-type")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
        if !write && status == StatusCode::TEMPORARY_REDIRECT {
            if let Some(location) = upstream.get("location").and_then(|s| s.to_str().ok()) {
                if let Some(rest) = location.strip_prefix(&(r.base_path.clone() + "/")) {
                    let parts: Vec<_> = rest.splitn(2, '?').collect();
                    if parts[0].starts_with("assets/")
                        && read_allowed(parts[0], parts.get(1).copied())
                    {
                        return Response::builder()
                            .status(status)
                            .header("location", format!("{}{}", mount(&id), rest))
                            .header("cache-control", "no-store")
                            .body(Body::empty())
                            .unwrap();
                    }
                }
            }
        }
        return fail(StatusCode::BAD_GATEWAY, "REVIEW_BOARD_REDIRECT_BLOCKED");
    }
    let mut builder = Response::builder()
        .status(status)
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer");
    for name in [
        "content-type",
        "cache-control",
        "etag",
        "last-modified",
        "accept-ranges",
        "content-range",
        "content-disposition",
        "vary",
    ] {
        if let Some(value) = upstream.get(name) {
            builder = builder.header(name, value);
        }
    }
    if path.starts_with("assets/") && status.is_success() {
        if response.content_length().is_some_and(|v| v > MAX_MEDIA) {
            return fail(StatusCode::PAYLOAD_TOO_LARGE, "REVIEW_BOARD_MEDIA_LIMIT");
        }
        builder = builder.header("content-security-policy", "default-src 'none'; sandbox");
        if let Some(n) = upstream.get("content-length") {
            builder = builder.header("content-length", n);
        }
        let stream = response.bytes_stream().scan(0u64, |size, chunk| {
            let result = match chunk {
                Ok(chunk) => {
                    *size += chunk.len() as u64;
                    if *size > MAX_MEDIA {
                        Err(std::io::Error::other("REVIEW_BOARD_MEDIA_LIMIT"))
                    } else {
                        Ok(chunk)
                    }
                }
                Err(_) => Err(std::io::Error::other("REVIEW_BOARD_MEDIA_UNCONFIRMED")),
            };
            std::future::ready(Some(result))
        });
        return builder.body(Body::from_stream(stream)).unwrap();
    }
    let bytes = match limited(response, MAX_TEXT).await {
        Ok(v) => v,
        Err(_) => {
            return fail(
                if write {
                    StatusCode::GATEWAY_TIMEOUT
                } else {
                    StatusCode::BAD_GATEWAY
                },
                "REVIEW_BOARD_RESPONSE_UNCONFIRMED",
            )
        }
    };
    let is_frame = (path.is_empty() || path == "index.html") && status.is_success();
    let mut output = bytes;
    if is_frame {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        output = match std::str::from_utf8(&output)
            .ok()
            .and_then(|t| frame_document(t, &r, &origin, &nonce).ok())
        {
            Some(v) => v.into_bytes(),
            None => return fail(StatusCode::BAD_GATEWAY, "REVIEW_BOARD_ENTRY_INVALID"),
        };
        let scripts = SCRIPTS
            .iter()
            .map(|name| format!("{origin}{}{name}", mount(&id)))
            .collect::<Vec<_>>()
            .join(" ");
        builder=builder.header("cache-control","no-store").header("content-security-policy",format!("default-src 'none'; script-src 'nonce-{nonce}' {scripts}; style-src 'unsafe-inline' {origin}{}; img-src {origin}{} data: blob:; media-src {origin}{} blob:; connect-src {origin}{}; font-src {origin}{} data:; base-uri 'self'; form-action 'none'; frame-src 'none'",mount(&id),mount(&id),mount(&id),mount(&id),mount(&id)));
    } else if !write && status.is_success() && mime.starts_with("application/json") {
        if let Ok(mut v) = serde_json::from_slice::<Value>(&output) {
            maps(&mut v, &r, &origin);
            output = serde_json::to_vec(&v).unwrap();
        }
    }
    builder.body(Body::from(output)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_and_range_boundaries() {
        for scope in [
            "",
            "?scope=illustrations",
            "?scope=ui-resources",
            "?scope=motions",
            "?scope=motion-review",
        ] {
            assert!(read_allowed("api/reviews", scope.strip_prefix('?')));
        }
        assert!(read_allowed("api/works", None));
        for (path, q) in [
            ("api/reviews", Some("scope=characters")),
            ("api/reviews", Some("scope=motions&scope=illustrations")),
            ("api/health", None),
            ("../auth", None),
            ("data/chunks/../../secret.json", None),
            ("server.mjs", None),
            ("data/approvals.json", None),
        ] {
            assert!(!read_allowed(path, q), "{path}");
        }
        assert_eq!(bounded_range("bytes=0-"), Some("bytes=0-4194303".into()));
        assert_eq!(bounded_range("bytes=-100"), Some("bytes=-100".into()));
        assert_eq!(bounded_range("bytes=30-60"), Some("bytes=30-60".into()));
        for bad in ["bytes=-0", "bytes=4-2", "bytes=0-1,5-6", "x=0-1"] {
            assert_eq!(bounded_range(bad), None);
        }
        assert!(write_allowed("api/draft", None));
        assert!(!write_allowed("api/draft", Some("scope=motions")));
        assert!(!write_allowed("api/post", None));
    }
    #[test]
    fn context_maps_transport_without_mutating_review_identity() {
        let r = Registration {
            id: "a".repeat(32),
            root: PathBuf::new(),
            title: "fixture".into(),
            port: 10000,
            base_path: "/board".into(),
            legacy_base: None,
        };
        let mut v = json!({"assetURLs":{"local":"/board/assets/abc"},"manifest":{"works":"/board/data/chunks/works.hash.json"},"path":"/board/original.png","fingerprint":"original","comment":"/board/comment stays","state":{"revision":12}});
        maps(&mut v, &r, "https://fixture.invalid");
        assert_eq!(v["path"], "/board/original.png");
        assert_eq!(v["comment"], "/board/comment stays");
        assert_eq!(v["state"]["revision"], 12);
        assert!(v["assetURLs"]["local"]
            .as_str()
            .unwrap()
            .starts_with(&mount(&r.id)));
    }
    #[test]
    #[ignore = "requires task-owned isolated original-service copy; never production data"]
    fn isolated_original_service_transport() {
        use crate::host_application::RouterCore;
        use crate::mobile_http::{start_with_web_auth, MobileHttpConfig};
        use std::sync::Arc;
        let scratch = PathBuf::from(
            std::env::var_os("AGBRIO_REVIEW_FIXTURE").expect("task-owned fixture required"),
        );
        let ready: Value =
            serde_json::from_slice(&fs::read(scratch.join("fixture-ready.json")).unwrap()).unwrap();
        let root = PathBuf::from(ready["root"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        assert!(root.starts_with(scratch.canonicalize().unwrap()));
        let port = url::Url::parse(ready["origin"].as_str().unwrap())
            .unwrap()
            .port()
            .unwrap();
        let temp = tempfile::tempdir_in(&scratch).unwrap();
        let registry = temp.path().join("review-registry.json");
        let id = "d".repeat(32);
        let row = Registration {
            id: id.clone(),
            root: root.clone(),
            title: "Isolated review".into(),
            port,
            base_path: "/board".into(),
            legacy_base: None,
        };
        save(&registry, &[row]).unwrap();
        *TEST_STORE.lock().unwrap() = Some(registry);
        let static_registry=temp.path().join("static-registry.json");fs::write(&static_registry,b"[]").unwrap();*crate::published_boards::TEST_STORE.lock().unwrap()=Some(static_registry);
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                *TEST_STORE.lock().unwrap() = None;*crate::published_boards::TEST_STORE.lock().unwrap()=None;
            }
        }
        let _reset = Reset;
        let chosen = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api_port = chosen.local_addr().unwrap().port();
        drop(chosen);
        let base = format!("http://127.0.0.1:{api_port}");
        let hostname = format!("127.0.0.1:{api_port}");
        let core = RouterCore {
            store: Arc::new(crate::RouterStore::open_at(temp.path().join("qa.db")).unwrap()),
            session: Default::default(),
            chatgpt: Default::default(),
            completed_chatgpt_responses: Default::default(),
        };
        let auth = Arc::new(crate::web_auth::WebAuth::open(None).unwrap());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let config = MobileHttpConfig {
            port: api_port,
            allowed_host: hostname,
            allowed_origin: base.clone(),
            access_issuer: "https://unused.invalid".into(),
            access_audience: "unused".into(),
            access_jwks_url: "https://unused.invalid/keys".into(),
            static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("dist"),
        };
        let _host = runtime
            .block_on(start_with_web_auth(
                core,
                config,
                crate::HostRuntime::default(),
                auth.clone(),
            ))
            .unwrap();
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let code = auth.issue().unwrap();
        let paired = client
            .post(format!("{base}/v1/mobile/auth/pair"))
            .header("origin", &base)
            .json(&json!({"code":code.code,"deviceName":"isolated fixture"}))
            .send()
            .unwrap();
        assert_eq!(paired.status(), 200);
        let cookie = paired.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let mount = format!("{base}{}", mount(&id));
        let get = |path: &str| {
            client
                .get(format!("{mount}{path}"))
                .header("cookie", &cookie)
                .send()
                .unwrap()
        };
        assert_eq!(client.get(&mount).send().unwrap().status(), 401);
        assert_eq!(get("server.mjs").status(), 403);
        assert_eq!(get("data/approvals.json").status(), 403);
        let frame = get("");
        assert_eq!(frame.status(), 200);
        let csp = frame.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("nonce-"));
        assert!(!csp.contains("script-src 'unsafe-inline'"));
        let html = frame.text().unwrap();
        assert!(html.contains(&format!("/review-boards/{id}/frame/")));
        assert!(!html.contains("<base href=\"http://127.0.0.1:47816"));
        assert_eq!(get("api/works").status(), 200);
        for name in SCRIPTS {
            assert_eq!(get(name).status(), 200, "{name}");
        }
        let source = ready["origin"].as_str().unwrap();
        let mut checks = Vec::new();
        for scope in [
            None,
            Some("illustrations"),
            Some("ui-resources"),
            Some("motions"),
            Some("motion-review"),
        ] {
            let query = scope.map(|s| format!("?scope={s}")).unwrap_or_default();
            let path = format!("api/reviews{query}");
            let original: Value = client
                .get(format!("{source}/board/{path}"))
                .send()
                .unwrap()
                .json()
                .unwrap();
            let snapshot: Value = get(&path).json().unwrap();
            assert_eq!(
                snapshot, original,
                "native ledger snapshots must remain exact: {scope:?}"
            );
            assert!(
                !snapshot["requests"].as_array().unwrap().is_empty(),
                "fixture missing {scope:?}"
            );
            let req = &snapshot["requests"][0];
            let key = req["key"].as_str().unwrap();
            let candidate = req["candidates"][0]["id"].as_str().unwrap();
            let mut body = json!({"revision":snapshot["state"]["revision"],"items":{key:{"fingerprint":req["fingerprint"],"disposition":"approve","candidateId":candidate,"comment":"isolated draft preserved"}},"reviewProtocol":"partial-v1","keys":[key]});
            if let Some(scope) = scope {
                body["scope"] = json!(scope);
            } else {
                body.as_object_mut().unwrap().remove("keys");
            }
            let send = |route: &str, body: &Value| {
                client
                    .post(format!("{mount}api/{route}"))
                    .header("cookie", &cookie)
                    .header("origin", &base)
                    .header("x-board-token", snapshot["csrf"].as_str().unwrap())
                    .json(body)
                    .send()
                    .unwrap()
            };
            assert_eq!(send("draft", &body).status(), 200, "draft {scope:?}");
            let saved: Value = get(&path).json().unwrap();
            assert_eq!(
                saved["state"]["draft"]["items"][key]["comment"],
                "isolated draft preserved"
            );
            body["revision"] = saved["state"]["revision"].clone();
            body["requestId"] = saved["state"]["draft"]["id"].clone();
            if scope.is_some() {
                body["keys"] = json!([key]);
            }
            body.as_object_mut().unwrap().remove("items");
            let submitted = send("submit", &body);
            assert_eq!(
                submitted.status(),
                200,
                "submit {scope:?}: {}",
                submitted.text().unwrap()
            );
            let result: Value = get(&path).json().unwrap();
            assert!(result["state"]["submissions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|i| i["key"] == key)));
            let submissions = result["state"]["submissions"].as_array().unwrap().len();
            let repeated = send("submit", &body);
            assert!(matches!(repeated.status().as_u16(), 200 | 409));
            assert_eq!(
                get(&path).json::<Value>().unwrap()["state"]["submissions"]
                    .as_array()
                    .unwrap()
                    .len(),
                submissions,
                "repeat must not duplicate ledger"
            );
            checks.push(format!("{scope:?}: GET/draft/submit/refresh/repeat"));
        }

        // Representation and media integrity, including the original oversized animated
        // WebP and suffix-range MP4 behavior, through the real authenticated proxy.
        let entry: Value = serde_json::from_str(
            &dom_query::Document::from(html.as_str())
                .select("#board-bootstrap")
                .text(),
        )
        .unwrap();
        let prefix = format!("/v1/mobile/review-boards/{id}/frame/");
        let bootstrap: Value = get(entry["manifest"]["bootstrap"]
            .as_str()
            .unwrap()
            .strip_prefix(&prefix)
            .unwrap())
        .json()
        .unwrap();
        let mut media = serde_json::Map::new();
        for path in bootstrap["bootstrap"]["manifest"]
            .as_object()
            .unwrap()
            .values()
        {
            let v: Value = get(path.as_str().unwrap().strip_prefix(&prefix).unwrap())
                .json()
                .unwrap();
            if let Some(urls) = v["assetURLs"].as_object() {
                media.extend(urls.clone());
            }
        }
        let mut media_bytes = 0usize;
        let mut saw_large = false;
        let mut saw_video = false;
        let mut saw_svg = false;
        for (file, url) in media {
            let file = PathBuf::from(file).canonicalize().unwrap();
            assert!(file.starts_with(&root));
            let relative = url.as_str().unwrap().strip_prefix(&prefix).unwrap();
            let mut response = get(relative);
            if response.status() == StatusCode::TEMPORARY_REDIRECT {
                let location = response.headers()["location"].to_str().unwrap().to_string();
                response = get(location.strip_prefix(&prefix).unwrap());
            }
            assert_eq!(response.status(), 200);
            let bytes = response.bytes().unwrap();
            assert_eq!(
                bytes.as_ref(),
                fs::read(&file).unwrap(),
                "original asset bytes changed"
            );
            media_bytes += bytes.len();
            saw_large |= bytes.len() > 8 * 1024 * 1024;
            if file.extension().is_some_and(|s| s == "svg") {
                saw_svg = true;
            }
            if file.extension().is_some_and(|s| s == "mp4") {
                saw_video = true;
                let range = client
                    .get(format!("{mount}{relative}"))
                    .header("cookie", &cookie)
                    .header("range", "bytes=-64")
                    .send()
                    .unwrap();
                assert_eq!(range.status(), 206);
                assert!(range.headers().contains_key("content-range"));
                assert_eq!(range.bytes().unwrap().as_ref(), &bytes[bytes.len() - 64..]);
            }
        }
        assert!(saw_large && saw_video && saw_svg);
        checks.push(format!(
            "raw animation/SVG/MP4 and >8MiB media: {media_bytes} bytes exact"
        ));
        // A partial resource batch must preserve accepted/rejected and numeric ledger
        // revision; an obsolete revision remains a 409, never an implicit retry.
        let current: Value = get("api/reviews?scope=illustrations").json().unwrap();
        let req = current["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["key"] == "illustration:direction")
            .unwrap();
        let key = "illustration:direction";
        let mut partial = json!({"scope":"illustrations","reviewProtocol":"partial-v1","revision":current["state"]["revision"],"keys":[key,"illustration:missing"],"items":{key:{"fingerprint":req["fingerprint"],"disposition":"approve","candidateId":req["candidates"][0]["id"],"comment":"partial test"}}});
        let post = |route: &str, value: &Value| {
            client
                .post(format!("{mount}api/{route}"))
                .header("cookie", &cookie)
                .header("origin", &base)
                .header("x-board-token", current["csrf"].as_str().unwrap())
                .json(value)
                .send()
                .unwrap()
        };
        let response = post("draft", &partial);
        assert_eq!(response.status(), 200);
        let saved: Value = response.json().unwrap();
        assert_eq!(saved["acceptedKeys"], json!([key]));
        assert_eq!(saved["rejected"][0]["key"], "illustration:missing");
        partial["revision"] = saved["state"]["revision"].clone();
        partial["requestId"] = saved["state"]["draft"]["id"].clone();
        let response = post("submit", &partial);
        assert_eq!(response.status(), 200);
        let submitted: Value = response.json().unwrap();
        assert_eq!(submitted["persisted"], true);
        assert_eq!(submitted["acceptedKeys"], json!([key]));
        assert_eq!(submitted["rejected"][0]["key"], "illustration:missing");
        partial["requestId"] = json!(uuid::Uuid::new_v4().to_string());
        assert_eq!(post("draft", &partial).status(), 409);
        checks.push("partial accepted/rejected/persisted and stale revision preserved".into());
        assert_eq!(
            client
                .post(format!("{mount}api/draft"))
                .header("cookie", &cookie)
                .header("origin", "https://unrelated.invalid")
                .json(&partial)
                .send()
                .unwrap()
                .status(),
            403
        );
        assert_eq!(
            client
                .post(format!("{mount}api/draft"))
                .header("cookie", &cookie)
                .header("origin", &base)
                .json(&partial)
                .send()
                .unwrap()
                .status(),
            403
        );
        fs::write(scratch.join("native-transport-proof.json"),serde_json::to_vec_pretty(&json!({"checks":checks,"originalServiceRoot":root,"productionApprovalsTouched":false,"deviceOnly":true,"status":"PASS"})).unwrap()).unwrap();
        if std::env::var_os("AGBRIO_REVIEW_BROWSER_HOLD").is_some() {
            let code = auth.issue().unwrap();
            fs::write(
                scratch.join("browser-ready.json"),
                serde_json::to_vec(&json!({"base":base,"mount":mount,"pairCode":code.code}))
                    .unwrap(),
            )
            .unwrap();
            for _ in 0..900 {
                if scratch.join("browser-finished").exists() {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
