//! Native Notification Center leaf. Authorization is requested only by an owner
//! command; passive reads/delivery never prompt or infer consent.
use block2::{DynBlock,RcBlock};
use objc2::{define_class,msg_send,AnyThread,DefinedClass,rc::Retained,runtime::{Bool,ProtocolObject}};
use objc2_foundation::{NSError,NSObject,NSObjectProtocol,NSString};
use objc2_user_notifications::*;
use std::{ptr::NonNull,sync::{mpsc,OnceLock},time::Duration};

struct Ivars { app:tauri::AppHandle }
define_class!(
    // NSObject has no subclassing requirements. The only ivar is a thread-safe
    // Tauri handle; all callbacks below obey the Objective-C protocol ABI.
    #[unsafe(super = NSObject)]
    #[thread_kind = AnyThread]
    #[name = "AgbrioNotificationDelegate"]
    #[ivars = Ivars]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn received(&self,_center:&UNUserNotificationCenter,response:&UNNotificationResponse,done:&DynBlock<dyn Fn()>){
            // Dismissal must not navigate. A default click carries only our
            // exact notification sequence; the normal retained-item read
            // still authorizes the resulting presentation.
            if &*response.actionIdentifier()==unsafe{UNNotificationDefaultActionIdentifier}{
                if let Some(sequence)=sequence(&response.notification().request().identifier().to_string()){
                    crate::watch_notifications::open_sequence(&self.ivars().app,sequence);
                }
            }
            done.call(());
        }
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn presenting(&self,_center:&UNUserNotificationCenter,_notification:&UNNotification,done:&DynBlock<dyn Fn(UNNotificationPresentationOptions)>){
            done.call((UNNotificationPresentationOptions::Banner|UNNotificationPresentationOptions::List|UNNotificationPresentationOptions::Sound,));
        }
    }
);
// Retain/release are thread-safe and the immutable delegate contains only the
// Send+Sync application handle. No main-thread-only native object is retained.
unsafe impl Send for Delegate {}
unsafe impl Sync for Delegate {}
static DELEGATE:OnceLock<Retained<Delegate>>=OnceLock::new();
pub(crate) fn init(app:&tauri::AppHandle){
    let delegate=DELEGATE.get_or_init(||{
        let allocated=Delegate::alloc().set_ivars(Ivars{app:app.clone()});
        unsafe{msg_send![super(allocated),init]}
    });
    UNUserNotificationCenter::currentNotificationCenter().setDelegate(Some(ProtocolObject::from_ref(&**delegate)));
}
fn sequence(identifier:&str)->Option<i64>{
    let value=identifier.strip_prefix("agbrio-watch-")?;
    if value.is_empty()||!value.bytes().all(|b|b.is_ascii_digit()){return None;}
    let number=value.parse::<i64>().ok()?;(number>=0&&number.to_string()==value).then_some(number)
}
fn authorization_status(value:UNAuthorizationStatus)->&'static str{
    if matches!(value,UNAuthorizationStatus::Authorized|UNAuthorizationStatus::Provisional){"ENABLED"}
    else if value==UNAuthorizationStatus::NotDetermined{"NOT_INITIALIZED"}
    else if value==UNAuthorizationStatus::Denied{"DISABLED_BY_MACOS"}
    else{"UNAVAILABLE"}
}
pub(crate) fn status()->String{
    if DELEGATE.get().is_none(){return "UNAVAILABLE".into();}
    let(tx,rx)=mpsc::sync_channel(1);
    let callback=RcBlock::new(move|settings:NonNull<UNNotificationSettings>|{let _=tx.try_send(authorization_status(unsafe{settings.as_ref()}.authorizationStatus()));});
    UNUserNotificationCenter::currentNotificationCenter().getNotificationSettingsWithCompletionHandler(&callback);
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or("UNAVAILABLE").into()
}
pub(crate) fn request_permission()->Result<(),String>{
    if DELEGATE.get().is_none(){return Err("MACOS_NOTIFICATION_UNAVAILABLE".into());}
    let(tx,rx)=mpsc::sync_channel(1);
    let callback=RcBlock::new(move|granted:Bool,error:*mut NSError|{let _=tx.try_send(granted.as_bool()&&error.is_null());});
    UNUserNotificationCenter::currentNotificationCenter().requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert|UNAuthorizationOptions::Sound|UNAuthorizationOptions::Badge,&callback);
    match rx.recv_timeout(Duration::from_secs(30)){Ok(true)=>Ok(()),Ok(false)=>Err("MACOS_NOTIFICATIONS_DISABLED".into()),Err(_)=>Err("MACOS_NOTIFICATION_PERMISSION_PENDING".into())}
}
pub(crate) fn show(event:Option<&router_core::store::codex_watch::WatchEvent>)->Result<(),String>{
    if status()!="ENABLED"{return Err("MACOS_NOTIFICATIONS_DISABLED".into());}
    let seq=event.map(|e|e.sequence).unwrap_or(0);if seq<0{return Err("MACOS_NOTIFICATION_INVALID".into());}
    let label=event.map(|e|e.label.as_str()).unwrap_or("通知通道测试");
    let label:String=label.chars().filter(|c|!c.is_control()).take(100).collect();
    let title=event.map(|e|match e.snapshot.state.as_str(){"ACTION_REQUIRED"=>"需要你确认或回答","FAILED"=>"任务执行失败",_=>"新结果已到达"}).unwrap_or("macOS 通知通道测试");
    let content=UNMutableNotificationContent::new();content.setTitle(&NSString::from_str(title));content.setBody(&NSString::from_str(&label));
    let request=UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&format!("agbrio-watch-{seq}")),&content,None);
    let(tx,rx)=mpsc::sync_channel(1);let callback=RcBlock::new(move|error:*mut NSError|{let _=tx.try_send(error.is_null());});
    UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request,Some(&callback));
    // Submission confirmation is channel acceptance, never proof that a human
    // saw/clicked it. A timeout stays UNKNOWN and must not cause a duplicate.
    match rx.recv_timeout(Duration::from_secs(3)){Ok(true)=>Ok(()),Ok(false)=>Err("MACOS_NOTIFICATION_REJECTED".into()),Err(_)=>Err("MACOS_NOTIFICATION_ACCEPTANCE_UNKNOWN".into())}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_permission_projection_does_not_invent_grants(){
        assert_eq!(authorization_status(UNAuthorizationStatus::NotDetermined),"NOT_INITIALIZED");assert_eq!(authorization_status(UNAuthorizationStatus::Denied),"DISABLED_BY_MACOS");assert_eq!(authorization_status(UNAuthorizationStatus::Authorized),"ENABLED");
    }
    #[test]fn click_routes_only_canonical_owned_event_identifiers(){
        assert_eq!(sequence("agbrio-watch-42"),Some(42));
        for value in ["42","agbrio-watch--1","agbrio-watch-01","agbrio-watch-42/other","agbrio-watch-9223372036854775808"]{assert_eq!(sequence(value),None);}
    }
}
