//! Notification adapters: bounded network work runs independently of observation.
use lettre::{transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
use router_core::store::{
    codex_watch::WatchEvent,
    watch_delivery::{DeliveryRecord, DeliverySettings},
    RouterStore,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};

const VAULT_RESOURCE: &str = "AI Work Router Gmail SMTP";
pub const AUMID: &str = "com.geoffrey.aiworkrouter";
#[derive(Default)]
pub struct NotificationNavigation(pub Mutex<Option<i64>>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: DeliverySettings,
    pub has_secret: bool,
    pub windows_status: String,
    pub history: Vec<DeliveryRecord>,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryCommand {
    Channel {
        channel: String,
        enabled: bool,
    },
    Email {
        account: String,
        recipient: String,
        password: Option<String>,
    },
    ForgetPassword,
    Test {
        channel: String,
    },
}
fn valid_addresses(account: &str, recipient: &str) -> Result<(), String> {
    for v in [account, recipient] {
        v.parse::<lettre::Address>()
            .map_err(|_| "EMAIL_ADDRESS_INVALID")?;
    }
    Ok(())
}
#[cfg(windows)]
fn password(account: &str) -> Result<String, String> {
    use windows::{core::HSTRING, Security::Credentials::PasswordVault};
    let vault = PasswordVault::new().map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE")?;
    let p = vault
        .Retrieve(&HSTRING::from(VAULT_RESOURCE), &HSTRING::from(account))
        .map_err(|_| "EMAIL_APP_PASSWORD_REQUIRED")?;
    p.RetrievePassword()
        .map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE")?;
    p.Password()
        .map(|s| s.to_string())
        .map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE".into())
}
#[cfg(windows)]
fn store_password(account: &str, secret: &str) -> Result<(), String> {
    use windows::{
        core::HSTRING,
        Security::Credentials::{PasswordCredential, PasswordVault},
    };
    // Gmail app passwords are 16 ASCII letters; spaces from Google's display are removed.
    let secret: String = secret
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if secret.len() != 16 || !secret.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err("EMAIL_APP_PASSWORD_FORMAT_INVALID".into());
    }
    let vault = PasswordVault::new().map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE")?;
    let p = PasswordCredential::CreatePasswordCredential(
        &HSTRING::from(VAULT_RESOURCE),
        &HSTRING::from(account),
        &HSTRING::from(secret),
    )
    .map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE")?;
    vault
        .Add(&p)
        .map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE".into())
}
#[cfg(windows)]
fn forget_password(account: &str) -> Result<(), String> {
    use windows::{core::HSTRING, Security::Credentials::PasswordVault};
    let vault = PasswordVault::new().map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE")?;
    if let Ok(p) = vault.Retrieve(&HSTRING::from(VAULT_RESOURCE), &HSTRING::from(account)) {
        vault
            .Remove(&p)
            .map_err(|_| "EMAIL_CREDENTIAL_STORE_UNAVAILABLE")?;
    }
    Ok(())
}
#[cfg(not(windows))]
fn password(_: &str) -> Result<String, String> {
    Err("EMAIL_CREDENTIAL_STORE_UNAVAILABLE".into())
}
#[cfg(not(windows))]
fn store_password(_: &str, _: &str) -> Result<(), String> {
    Err("EMAIL_CREDENTIAL_STORE_UNAVAILABLE".into())
}
#[cfg(not(windows))]
fn forget_password(_: &str) -> Result<(), String> {
    Ok(())
}

pub fn view(store: &RouterStore) -> Result<SettingsView, String> {
    let settings = store.watch_delivery_settings()?;
    let has_secret = password(&settings.account).is_ok();
    Ok(SettingsView {
        settings,
        has_secret,
        windows_status: windows_status(),
        history: store.watch_delivery_history()?,
    })
}
pub fn command(store: &RouterStore, input: DeliveryCommand) -> Result<String, String> {
    match input {
        DeliveryCommand::Channel { channel, enabled } => {
            if enabled && channel == "EMAIL" {
                let s = store.watch_delivery_settings()?;
                valid_addresses(&s.account, &s.recipient)?;
                password(&s.account)?;
                notification_url(Some(1))?;
            }
            if enabled && channel == "WINDOWS" && windows_status() != "ENABLED" {
                return Err("WINDOWS_NOTIFICATIONS_DISABLED".into());
            }
            store.set_watch_delivery_channel(&channel, enabled)?;
            Ok("SAVED".into())
        }
        DeliveryCommand::Email {
            account,
            recipient,
            password: secret,
        } => {
            valid_addresses(&account, &recipient)?;
            let before = store.watch_delivery_settings()?;
            if account != before.account
                && before
                    .channels
                    .iter()
                    .any(|c| c.channel == "EMAIL" && c.enabled)
            {
                store.set_watch_delivery_channel("EMAIL", false)?;
            }
            if let Some(secret) = secret.filter(|v| !v.is_empty()) {
                store_password(&account, &secret)?;
            }
            store.set_watch_email_addresses(&account, &recipient)?;
            Ok("SAVED".into())
        }
        DeliveryCommand::ForgetPassword => {
            store.set_watch_delivery_channel("EMAIL", false)?;
            forget_password(&store.watch_delivery_settings()?.account)?;
            Ok("REMOVED".into())
        }
        DeliveryCommand::Test { channel } => {
            let s = store.watch_delivery_settings()?;
            let id = format!("aiwr-{}-test-{}", s.host_id, uuid::Uuid::new_v4());
            let (status, code) = deliver(&channel, &s, &id, None);
            if status == "SENT" {
                Ok("ACCEPTED".into())
            } else {
                Err(code.unwrap_or("DELIVERY_FAILED").into())
            }
        }
    }
}
pub fn notification_url(sequence: Option<i64>) -> Result<String, String> {
    let origin = crate::mobile_http::configured_mobile_allowed_origin()
        .ok_or("NOTIFICATION_WEB_URL_UNAVAILABLE")?;
    let mut u = url::Url::parse(&origin).map_err(|_| "NOTIFICATION_WEB_URL_UNAVAILABLE")?;
    if u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err("NOTIFICATION_WEB_URL_UNAVAILABLE".into());
    }
    u.set_path("/mobile/notifications");
    u.set_query(None);
    u.set_fragment(None);
    if let Some(seq) = sequence {
        u.query_pairs_mut().append_pair("event", &seq.to_string());
    }
    Ok(u.to_string())
}
fn email_message(
    s: &DeliverySettings,
    id: &str,
    event: Option<&WatchEvent>,
    link: &str,
) -> Result<Message, String> {
    valid_addresses(&s.account, &s.recipient)?;
    let title = event.map(|e| e.label.as_str()).unwrap_or("通知通道测试");
    let state = event.map(|e| e.snapshot.state.as_str()).unwrap_or("TEST");
    let title: String = title
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect();
    let body = if let Some(e) = event {
        format!("AI Work Router 事件通知\n事件 ID: {id}\n通知序号: {}\n事件类型: {}\n对话名称: {}\n对话 ID: {}\n任务/Turn ID: {}\n项目路径: {}\n发生时间 (Unix 毫秒): {}\n通知原文链接: {link}\n\n请在 Router 登录后查看完整原文。此邮件不会批准或执行任务。",e.sequence,e.snapshot.state,e.label,e.thread_id,e.snapshot.turn_id.as_deref().unwrap_or("未提供"),e.cwd,e.observed_at)
    } else {
        format!("AI Work Router 邮件通道测试\n事件 ID: {id}\n通知入口: {link}\n\n这是一封测试邮件，没有任务结果。")
    };
    Message::builder()
        .from(s.account.parse().map_err(|_| "EMAIL_ADDRESS_INVALID")?)
        .to(s.recipient.parse().map_err(|_| "EMAIL_ADDRESS_INVALID")?)
        .subject(format!("[AIWR_EVENT] {state} · {title}"))
        .message_id(Some(format!("<{id}@ai-work-router.local>")))
        .body(body)
        .map_err(|_| "EMAIL_MESSAGE_INVALID".into())
}
fn deliver(
    channel: &str,
    s: &DeliverySettings,
    id: &str,
    event: Option<&WatchEvent>,
) -> (&'static str, Option<&'static str>) {
    match channel {
        "WINDOWS" => match show_windows(event) {
            Ok(()) => ("SENT", None),
            Err(_) => ("SKIPPED", Some("WINDOWS_NOTIFICATION_REJECTED")),
        },
        "WEB" => {
            let payload = serde_json::json!({"type":"codex_watch","eventId":id,"sequence":event.map(|e|e.sequence),"label":event.map(|e|e.label.as_str()).unwrap_or("通知通道测试"),"state":event.map(|e|e.snapshot.state.as_str()).unwrap_or("TEST")});
            match crate::push::send_payload(payload.to_string().as_bytes()) {
                Ok(crate::push::PushDeliveryOutcome::Sent { .. }) => ("SENT", None),
                Ok(crate::push::PushDeliveryOutcome::NoSubscription) => {
                    ("SKIPPED", Some("WEB_PUSH_NO_SUBSCRIPTION"))
                }
                _ => ("UNKNOWN", Some("WEB_PUSH_NOT_CONFIRMED")),
            }
        }
        "EMAIL" => {
            let Ok(secret) = password(&s.account) else {
                return ("SKIPPED", Some("EMAIL_APP_PASSWORD_REQUIRED"));
            };
            let Ok(link) = notification_url(event.map(|e| e.sequence)) else {
                return ("SKIPPED", Some("NOTIFICATION_WEB_URL_UNAVAILABLE"));
            };
            let Ok(message) = email_message(s, id, event, &link) else {
                return ("SKIPPED", Some("EMAIL_MESSAGE_INVALID"));
            };
            let Ok(transport) = SmtpTransport::relay("smtp.gmail.com") else {
                return ("SKIPPED", Some("SMTP_TLS_UNAVAILABLE"));
            };
            let transport = transport
                .credentials(Credentials::new(s.account.clone(), secret))
                .timeout(Some(Duration::from_secs(12)))
                .build();
            smtp_outcome(transport.send(&message))
        }
        _ => ("SKIPPED", Some("DELIVERY_CHANNEL_INVALID")),
    }
}
fn smtp_outcome(
    result: Result<lettre::transport::smtp::response::Response, lettre::transport::smtp::Error>,
) -> (&'static str, Option<&'static str>) {
    match result {
        Ok(_) => ("SENT", None),
        Err(e) if e.is_transient() => ("FAILED", Some("SMTP_TEMPORARILY_REJECTED")),
        Err(e) if e.is_permanent() => ("SKIPPED", Some("SMTP_REJECTED_CHECK_ACCOUNT")),
        // Connection/timeout after DATA can mean accepted mail; never blind-retry.
        Err(_) => ("UNKNOWN", Some("SMTP_ACCEPTANCE_UNKNOWN")),
    }
}
pub fn start(store: Arc<RouterStore>) {
    std::thread::spawn(move || {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(
                windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
            );
        }
        let _ = store.recover_watch_deliveries();
        loop {
            #[cfg(windows)]
            let _ = repair_legacy_toasts(&store);
            let _ = store.enqueue_watch_deliveries();
            if let Ok(Some((channel, event))) = store.claim_watch_delivery() {
                if let Ok(s) = store.watch_delivery_settings() {
                    let id = format!("aiwr-{}-{}", s.host_id, event.sequence);
                    let _=deliver_claimed_event_with(&store,&channel,&event,||deliver(&channel,&s,&id,Some(&event)));
                } else {
                    let _ = store.finish_watch_delivery(
                        event.sequence,
                        &channel,
                        "FAILED",
                        Some("SETTINGS_UNAVAILABLE"),
                    );
                }
            } else {
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    });
}

fn deliver_claimed_event_with(store:&RouterStore,channel:&str,event:&WatchEvent,send:impl FnOnce()->(&'static str,Option<&'static str>))->Result<(),String>{
    if !store.watch_event_delivery_enabled(&event.thread_id)?{return store.finish_watch_delivery(event.sequence,channel,"SKIPPED",Some("WATCH_DISABLED_OR_BOUND"));}
    let(status,code)=send();store.finish_watch_delivery(event.sequence,channel,status,code)
}

pub fn sequence_from_url(input: &str) -> Option<i64> {
    let u = url::Url::parse(input).ok()?;
    if u.scheme() != "ai-work-router"
        || u.host_str() != Some("notifications")
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || u.fragment().is_some()
        || !matches!(u.path(), "" | "/")
    {
        return None;
    }
    let pairs = u.query_pairs().collect::<Vec<_>>();
    if pairs.len() != 1 || pairs[0].0 != "event" {
        return None;
    }
    let seq = pairs[0].1.parse::<i64>().ok()?;
    (seq >= 0).then_some(seq)
}
pub fn navigate(app: &tauri::AppHandle, args: impl IntoIterator<Item = String>) {
    if let Some(seq) = args.into_iter().find_map(|arg| sequence_from_url(&arg)) {
        open_sequence(app, seq);
    }
}
pub fn open_sequence(app: &tauri::AppHandle, seq: i64) {
    // Activation only requests presentation. The authenticated event read still
    // verifies retained identity; an expired notification must foreground the app
    // and show its missing-record notice instead of silently doing nothing.
    if let Ok(mut pending) = app.state::<NotificationNavigation>().0.lock() {
        *pending = Some(seq);
    }
    let _ = app.emit("codex-notification-open", seq);
    crate::show_dashboard(app);
}
#[cfg(windows)]
pub fn register_windows() -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(format!(r"Software\Classes\AppUserModelId\{AUMID}"))
        .map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    key.set_value("DisplayName", &"Agbrio")
        .map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    let exe = std::env::current_exe().map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    if let Some(directory) = exe.parent() {
        let icon = directory.join("dist/assets/brand/app-192-v1.png");
        if icon.is_file() {
            key.set_value("IconUri", &icon.to_string_lossy().as_ref())
                .map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
        }
    }
    let (key, _) = hkcu
        .create_subkey(r"Software\Classes\ai-work-router")
        .map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    key.set_value("", &"URL:Agbrio notifications")
        .map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    key.set_value("URL Protocol", &"")
        .map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    let open_command = format!("\"{}\" \"%1\"", exe.display());
    let (command, _) = key.create_subkey(r"shell\open\command").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    command.set_value("", &open_command).map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    // WinRT toast protocol discovery also needs an owned ProgID/capability map.
    // Never write UserChoice or select a global/default browser.
    let progid = "com.geoffrey.aiworkrouter.Protocol";
    let (program, _) = hkcu.create_subkey(format!(r"Software\Classes\{progid}")).map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    program.set_value("", &"Agbrio notification").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    program.set_value("URL Protocol", &"").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    let (open, _) = program.create_subkey(r"shell\open\command").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    open.set_value("", &open_command).map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    let (capabilities, _) = hkcu.create_subkey(r"Software\AIWorkRouter\Capabilities").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    capabilities.set_value("ApplicationName", &"Agbrio").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    capabilities.set_value("ApplicationDescription", &"Open AI Work Router notifications").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    let (associations, _) = capabilities.create_subkey("URLAssociations").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    associations.set_value("ai-work-router", &progid).map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    let (registered, _) = hkcu.create_subkey(r"Software\RegisteredApplications").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    registered.set_value("Agbrio", &r"Software\AIWorkRouter\Capabilities").map_err(|_| "WINDOWS_NOTIFICATION_REGISTRATION_FAILED")?;
    unsafe { windows::Win32::UI::Shell::SHChangeNotify(windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED, windows::Win32::UI::Shell::SHCNF_IDLIST, None, None); }
    Ok(())
}
#[cfg(not(windows))]
pub fn register_windows() -> Result<(), String> {
    Ok(())
}
#[cfg(windows)]
fn windows_status() -> String {
    use windows::{
        core::HSTRING,
        UI::Notifications::{NotificationSetting, ToastNotificationManager},
    };
    match ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))
        .and_then(|n| n.Setting())
    {
        Ok(s) if s == NotificationSetting::Enabled => "ENABLED",
        Ok(_) => "DISABLED_BY_WINDOWS",
        // Win32's settings row may not exist until its first accepted toast.
        // Missing settings is neither permission denial nor delivery acceptance.
        Err(e) if e.code().0 as u32 == 0x80070490 => "NOT_INITIALIZED",
        Err(_) => "UNAVAILABLE",
    }
    .into()
}
#[cfg(not(windows))]
fn windows_status() -> String {
    "UNSUPPORTED".into()
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
#[cfg(windows)]
fn show_windows(event: Option<&WatchEvent>) -> Result<(), String> {
    show_windows_inner(event, false)
}
#[cfg(windows)]
fn show_windows_inner(event: Option<&WatchEvent>, silent: bool) -> Result<(), String> {
    use windows::{
        core::HSTRING,
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
    };
    if !matches!(windows_status().as_str(), "ENABLED" | "NOT_INITIALIZED") {
        return Err("WINDOWS_NOTIFICATIONS_DISABLED".into());
    }
    let seq = event.map(|e| e.sequence).unwrap_or(0);
    let label = event.map(|e| e.label.as_str()).unwrap_or("通知通道测试");
    let kind = event
        .map(|e| {
            if e.snapshot.state == "ACTION_REQUIRED" {"Codex 需要你确认或回答"}
            else if e.snapshot.state == "FAILED" {
                "Codex 执行失败"
            } else {
                "Codex 新结果已到达"
            }
        })
        .unwrap_or("Windows 通知通道测试");
    let launch = format!("ai-work-router://notifications?event={seq}");
    let body = windows_toast_xml(kind, label, &launch);
    let result = (|| -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(body))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        toast.SetTag(&HSTRING::from(format!("watch-{seq}")))?;
        toast.SetGroup(&HSTRING::from("codex-watches"))?;
        toast.SetSuppressPopup(silent)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
    })();
    result.map_err(|_| "WINDOWS_NOTIFICATION_REJECTED".into())
}
/// Old custom-scheme toasts remain clickable after an upgrade. Replace only our
/// matching app/group/tag entries, silently, from the exact retained event.
#[cfg(windows)]
pub fn repair_legacy_toasts(store: &RouterStore) -> Result<usize, String> {
    use windows::{core::HSTRING, UI::Notifications::ToastNotificationManager};
    let history = ToastNotificationManager::History().map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?;
    let entries = history
        .GetHistoryWithId(&HSTRING::from(AUMID))
        .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?;
    let mut repaired = 0;
    for n in 0..entries
        .Size()
        .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?
        .min(100)
    {
        let entry = entries
            .GetAt(n)
            .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?;
        if entry
            .Group()
            .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?
            .to_string()
            != "codex-watches"
        {
            continue;
        }
        let tag = entry
            .Tag()
            .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?
            .to_string();
        let Some(sequence) = tag
            .strip_prefix("watch-")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|s| *s >= 0)
        else {
            continue;
        };
        let node = entry
            .Content()
            .and_then(|c| c.DocumentElement())
            .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?;
        if !matches!(
            node.GetAttribute(&HSTRING::from("activationType"))
                .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?
                .to_string()
                .as_str(),
            "protocol" | "foreground"
        ) {
            continue;
        }
        let launch = node
            .GetAttribute(&HSTRING::from("launch"))
            .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?
            .to_string();
        let legacy = sequence_from_url(&launch) == Some(sequence);
        let foreground = node.GetAttribute(&HSTRING::from("activationType")).map_err(|_|"WINDOWS_HISTORY_UNAVAILABLE")?.to_string()=="foreground";
        let current = notification_url((sequence > 0).then_some(sequence))
            .ok()
            .as_deref()
            == Some(launch.as_str());
        if !legacy && !current {
            continue;
        }
        let event = if sequence == 0 {
            None
        } else {
            match store.codex_watch_event(sequence) {
                Ok(event) => Some(event),
                Err(error) if error == "WATCH_EVENT_NOT_FOUND" => {
                    history
                        .RemoveGroupedTagWithId(
                            &HSTRING::from(&tag),
                            &HSTRING::from("codex-watches"),
                            &HSTRING::from(AUMID),
                        )
                        .map_err(|_| "WINDOWS_HISTORY_UNAVAILABLE")?;
                    continue;
                }
                Err(error) => return Err(error),
            }
        };
        if legacy && !foreground {
            continue;
        }
        if event.as_ref().is_some_and(|e|!store.watch_event_delivery_enabled(&e.thread_id).unwrap_or(false)){continue;}
        show_windows_inner(event.as_ref(), true)?;
        repaired += 1;
    }
    Ok(repaired)
}
fn windows_toast_xml(kind: &str, label: &str, launch: &str) -> String {
    format!("<toast activationType=\"protocol\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text><text>点击在 AI Work Router 查看原文并回复</text></binding></visual></toast>",xml(launch),xml(kind),xml(&label.chars().take(120).collect::<String>()))
}
#[cfg(not(windows))]
fn show_windows(_: Option<&WatchEvent>) -> Result<(), String> {
    Err("WINDOWS_UNSUPPORTED".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn a_watch_paused_after_claim_never_reaches_the_physical_delivery_callback(){
        let d=tempfile::tempdir().unwrap();let store=RouterStore::open_at(d.path().join("router.db")).unwrap();let base=router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};store.enable_codex_watch("fixture-thread","fixture",d.path().to_str().unwrap(),&base).unwrap();store.set_watch_delivery_channel("EMAIL",true).unwrap();let result=router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some("turn".into()),item_id:Some("item".into()),text:"fictional".into()};store.record_codex_watch("fixture-thread",1,&result).unwrap();store.enqueue_watch_deliveries().unwrap();let(channel,event)=store.claim_watch_delivery().unwrap().unwrap();store.pause_codex_watch("fixture-thread").unwrap();let mut calls=0;deliver_claimed_event_with(&store,&channel,&event,||{calls+=1;("SENT",None)}).unwrap();assert_eq!(calls,0);assert!(store.watch_delivery_history().unwrap().iter().any(|d|d.status=="SKIPPED"&&d.attempts==1));
    }
    #[test]
    fn windows_alert_activates_desktop_original_without_browser_protocol() {
        let body = windows_toast_xml(
            "Codex 新结果",
            "public fixture",
            "ai-work-router://notifications?event=47",
        );
        assert!(body.contains("activationType=\"protocol\""));
        assert!(body.contains("launch=\"ai-work-router://notifications?event=47\""));
        assert!(!body.contains("activationType=\"foreground\""));
        assert!(windows_toast_xml(
            "kind",
            "label",
            "https://router.example/mobile/notifications?event=47&x=1"
        )
        .contains("&amp;x=1"));
    }
    #[test]
    fn activation_accepts_only_exact_local_event() {
        assert_eq!(
            sequence_from_url("ai-work-router://notifications?event=7"),
            Some(7)
        );
        for url in [
            "https://notifications?event=7",
            "ai-work-router://notifications?event=-1",
            "ai-work-router://notifications/path?event=7",
            "ai-work-router://notifications?event=1&event=2",
            "ai-work-router://user@notifications?event=1",
            "ai-work-router://notifications?event=1#extra",
        ] {
            assert_eq!(sequence_from_url(url), None);
        }
    }
    #[test]
    fn event_email_has_stable_identity_and_no_original_result() {
        let s = DeliverySettings {
            host_id: "host".into(),
            account: "sender@gmail.com".into(),
            recipient: "geoffreyzjx826@gmail.com".into(),
            channels: vec![],
        };
        let e = WatchEvent {
            sequence: 9,
            thread_id: "thread-exact".into(),
            label: "a\r\nb <script>".into(),
            cwd: "D:\\project".into(),
            snapshot: router_core::store::codex_watch::WatchSnapshot {
                state: "RESULT_READY".into(),
                turn_id: Some("turn-exact".into()),
                item_id: None,
                text: "SECRET_RESULT_NOT_FOR_EMAIL".into(),
            },
            observed_at: 123,seen_at:None,
        };
        let m = email_message(
            &s,
            "aiwr-host-9",
            Some(&e),
            "https://router.example/mobile/notifications?event=9",
        )
        .unwrap();
        let bytes = m.formatted();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("<aiwr-host-9@ai-work-router.local>"));
        assert!(!text.contains("SECRET_RESULT_NOT_FOR_EMAIL"));
        assert!(email_message(&s, "test", None, "https://router.example").is_ok());
    }
    #[test]
    fn loopback_smtp_delivery_acceptance_transient_and_permanent_rejections() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };
        for (reply, expected) in [(250, "SENT"), (451, "FAILED"), (550, "SKIPPED")] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                stream.write_all(b"220 fixture ESMTP\r\n").unwrap();
                let mut body = String::new();
                let mut in_data = false;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if in_data {
                        if line == ".\r\n" {
                            in_data = false;
                            write!(stream, "{reply} fixture response\r\n").unwrap();
                        } else {
                            body.push_str(&line);
                        }
                        continue;
                    }
                    if line.starts_with("EHLO") {
                        stream.write_all(b"250 fixture\r\n").unwrap();
                    } else if line.starts_with("DATA") {
                        in_data = true;
                        stream.write_all(b"354 send data\r\n").unwrap();
                    } else if line.starts_with("QUIT") {
                        stream.write_all(b"221 bye\r\n").unwrap();
                        break;
                    } else {
                        stream.write_all(b"250 accepted\r\n").unwrap();
                    }
                }
                body
            });
            let settings = DeliverySettings {
                host_id: "fixture".into(),
                account: "sender@gmail.com".into(),
                recipient: "geoffreyzjx826@gmail.com".into(),
                channels: vec![],
            };
            let message = email_message(
                &settings,
                "fixture-stable-id",
                None,
                "https://router.example/mobile/notifications",
            )
            .unwrap();
            // Plain SMTP is confined to this isolated loopback fixture. Production uses mandatory TLS relay.
            let transport = SmtpTransport::builder_dangerous("127.0.0.1")
                .port(port)
                .timeout(Some(Duration::from_secs(3)))
                .build();
            assert_eq!(smtp_outcome(transport.send(&message)).0, expected);
            let body = server.join().unwrap();
            assert!(body.contains("Message-ID: <fixture-stable-id@ai-work-router.local>"));
            assert!(body.contains("To: geoffreyzjx826@gmail.com"));
        }
    }
}
