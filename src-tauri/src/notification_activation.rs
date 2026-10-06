//! Win32 COM toast activation, following Microsoft's DesktopToasts sample.
//! No system-default override and no provider execution authority.
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::{
    core::{implement, IUnknown, Interface, Ref, BOOL, GUID, HSTRING, PCWSTR},
    Win32::{
        System::Com::StructuredStorage::{InitPropVariantFromCLSID, PropVariantClear},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoRegisterClassObject, IClassFactory,
            IClassFactory_Impl, IPersistFile, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER,
            COINIT_MULTITHREADED, REGCLS_MULTIPLEUSE, STGM_READWRITE,
        },
        UI::{
            Notifications::{
                INotificationActivationCallback, INotificationActivationCallback_Impl,
                NOTIFICATION_USER_INPUT_DATA,
            },
            Shell::{
                IShellLinkW, PropertiesSystem::IPropertyStore, SHChangeNotify, ShellLink,
                SHCNE_ASSOCCHANGED, SHCNF_IDLIST,
            },
        },
    },
};
pub const CLSID: GUID = GUID::from_u128(0x5c2b4cbf_2792_4d15_a116_5450505d7b23);
pub const CLSID_TEXT: &str = "{5C2B4CBF-2792-4D15-A116-5450505D7B23}";

/// Keep taskbar relaunch identity aligned with the owned branded shortcut.
pub fn install_window_identity(window: &tauri::WebviewWindow) -> windows::core::Result<()> {
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::{PropertiesSystem::SHGetPropertyStoreForWindow, SHStrDupW};
    let raw = window.hwnd().map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32)))?;
    let hwnd = windows::Win32::Foundation::HWND(raw.0);
    let exe = std::env::current_exe().map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32)))?;
    let icon = exe.parent().unwrap_or_else(|| std::path::Path::new(".")).join("dist").join("icon.ico");
    let properties: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(hwnd)? };
    let values = [format!("\"{}\"", exe.display()), format!("{},0", icon.display()), "Agbrio".into(), crate::watch_notifications::AUMID.into()];
    for (index, text) in values.iter().enumerate() {
        let key = PROPERTYKEY { fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: index as u32 + 2 };
        let mut value = PROPVARIANT { Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0, wReserved2: 0, wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { pwszVal: unsafe { SHStrDupW(&HSTRING::from(text))? } },
            }),
        }};
        let result = unsafe { properties.SetValue(&key, &value) };
        let _ = unsafe { PropVariantClear(&mut value) };
        result?;
    }
    unsafe { properties.Commit() }
}
static READY: AtomicBool = AtomicBool::new(false);
pub fn ready() -> bool {
    READY.load(Ordering::Acquire)
}
type Open = Arc<dyn Fn(i64) + Send + Sync>;

unsafe fn bounded_string(value: &PCWSTR, max: usize) -> Option<String> {
    if value.is_null() {
        return None;
    }
    for n in 0..=max {
        if unsafe { *value.0.add(n) } == 0 {
            return String::from_utf16(unsafe { std::slice::from_raw_parts(value.0, n) }).ok();
        }
    }
    None
}
#[implement(INotificationActivationCallback)]
struct Activator {
    open: Open,
}
impl INotificationActivationCallback_Impl for Activator_Impl {
    fn Activate(
        &self,
        app: &PCWSTR,
        args: &PCWSTR,
        _data: *const NOTIFICATION_USER_INPUT_DATA,
        count: u32,
    ) -> windows::core::Result<()> {
        if count != 0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80070057u32 as i32,
            )));
        }
        let app = unsafe { bounded_string(app, 128) };
        let args = unsafe { bounded_string(args, 1024) };
        if app.as_deref() != Some(crate::watch_notifications::AUMID) {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80070057u32 as i32,
            )));
        }
        let seq = args
            .as_deref()
            .and_then(crate::watch_notifications::sequence_from_url)
            .ok_or_else(|| {
                windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057u32 as i32))
            })?;
        (self.open)(seq);
        Ok(())
    }
}
#[implement(IClassFactory)]
struct Factory {
    open: Open,
}
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        iid: *const GUID,
        result: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if iid.is_null() || result.is_null() {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80004003u32 as i32,
            )));
        }
        unsafe {
            *result = std::ptr::null_mut();
        }
        if outer.as_ref().is_some() {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80040110u32 as i32,
            )));
        }
        let object: INotificationActivationCallback = Activator {
            open: self.open.clone(),
        }
        .into();
        unsafe { object.query(iid, result).ok() }
    }
    fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Register only this app's COM local server and the existing owned NSIS shortcut.
pub fn register(app: &tauri::AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|_| "WINDOWS_ACTIVATOR_REGISTRATION_FAILED")?;
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (server, _) = hkcu
        .create_subkey(format!(
            r"Software\Classes\CLSID\{CLSID_TEXT}\LocalServer32"
        ))
        .map_err(|_| "WINDOWS_ACTIVATOR_REGISTRATION_FAILED")?;
    server
        .set_value(
            "",
            &format!("\"{}\" --notification-activated", exe.display()),
        )
        .map_err(|_| "WINDOWS_ACTIVATOR_REGISTRATION_FAILED")?;
    server.set_value("ServerExecutable", &exe.to_string_lossy().as_ref())
        .map_err(|_| "WINDOWS_ACTIVATOR_REGISTRATION_FAILED")?;
    let (identity, _) = hkcu
        .create_subkey(format!(
            r"Software\Classes\AppUserModelId\{}",
            crate::watch_notifications::AUMID
        ))
        .map_err(|_| "WINDOWS_ACTIVATOR_REGISTRATION_FAILED")?;
    identity
        .set_value("CustomActivator", &CLSID_TEXT)
        .map_err(|_| "WINDOWS_ACTIVATOR_REGISTRATION_FAILED")?;
    let app = app.clone();
    let open: Open = Arc::new(move |seq| {
        let target = app.clone();
        let _ =
            app.run_on_main_thread(move || crate::watch_notifications::open_sequence(&target, seq));
    });
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| -> windows::core::Result<u32> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            }
            // Development fixtures have no installed shortcut. Their registry
            // and live COM callback can still be verified without replacing the
            // owner's installed shortcut. Normal installed runs verify it.
            if exe.parent().is_some_and(|p|p.join("uninstall.exe").is_file()) {
                install_shortcut_identity(&exe)?;
            }
            let factory: IClassFactory = Factory { open }.into();
            unsafe {
                CoRegisterClassObject(&CLSID, &factory, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)
            }
        })();
        let registered = result.is_ok();
        READY.store(registered, Ordering::Release);
        let _ = ready_tx.send(registered);
        // Keep the MTA registered for this resident Host's lifetime. OS process exit
        // releases the class registration; a future click starts the local server.
        if registered {
            loop {
                std::thread::park();
            }
        }
    });
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => Ok(()),
        _ => Err("WINDOWS_ACTIVATOR_REGISTRATION_FAILED".into()),
    }
}
fn install_shortcut_identity(exe: &std::path::Path) -> windows::core::Result<()> {
    let root = std::env::var_os("APPDATA").ok_or_else(|| {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x80070002u32 as i32))
    })?;
    let programs = std::path::PathBuf::from(root).join("Microsoft/Windows/Start Menu/Programs");
    let branded = programs.join("Agbrio/Agbrio.lnk");
    // Existing installs keep working until the branded installer updates the shortcut.
    let path = if branded.is_file() { branded } else {
        programs.join("AI Work Router/AI Work Router.lnk")
    };
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    let file: IPersistFile = link.cast()?;
    unsafe {
        file.Load(
            &HSTRING::from(path.to_string_lossy().as_ref()),
            STGM_READWRITE,
        )?;
    }
    let mut actual = [0u16; 32768];
    unsafe {
        link.GetPath(&mut actual, std::ptr::null_mut(), 0)?;
    }
    let length = actual.iter().position(|v| *v == 0).unwrap_or(actual.len());
    let actual = String::from_utf16_lossy(&actual[..length]);
    if !actual.eq_ignore_ascii_case(&exe.to_string_lossy()) {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070057u32 as i32,
        )));
    }
    unsafe {
        link.SetIconLocation(&HSTRING::from(exe.to_string_lossy().as_ref()), 0)?;
    }
    let properties: IPropertyStore = link.cast()?;
    let key = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 26,
    };
    let mut value = unsafe { InitPropVariantFromCLSID(&CLSID) }?;
    let result = unsafe {
        properties
            .SetValue(&key, &value)
            .and_then(|_| properties.Commit())
            .and_then(|_| file.Save(None, true))
    };
    let _ = unsafe { PropVariantClear(&mut value) };
    if result.is_ok() {
        unsafe {
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_rejects_foreign_identity_bad_arguments_and_input() {
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let values = received.clone();
        let callback: INotificationActivationCallback = Activator {
            open: Arc::new(move |n| values.lock().unwrap().push(n)),
        }
        .into();
        let app = HSTRING::from(crate::watch_notifications::AUMID);
        let good = HSTRING::from("ai-work-router://notifications?event=47");
        unsafe { callback.Activate(&app, &good, &[]) }.unwrap();
        assert!(unsafe { callback.Activate(&HSTRING::from("foreign"), &good, &[]) }.is_err());
        assert!(unsafe {
            callback.Activate(
                &app,
                &HSTRING::from("ai-work-router://notifications?event=1&event=2"),
                &[],
            )
        }
        .is_err());
        assert!(unsafe {
            callback.Activate(&app, &good, &[NOTIFICATION_USER_INPUT_DATA::default()])
        }
        .is_err());
        assert_eq!(*received.lock().unwrap(), vec![47]);
    }
}
