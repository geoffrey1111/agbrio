//! Trial eligibility only. Raw IOPlatformUUID never enters storage or telemetry.
use std::{ffi::{c_char, c_void, CStr}, ptr};
type CF = *const c_void;
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CF;
    fn IOServiceGetMatchingService(port: u32, matching: CF) -> u32;
    fn IORegistryEntryCreateCFProperty(entry: u32, key: CF, allocator: CF, options: u32) -> CF;
    fn IOObjectRelease(object: u32) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(allocator: CF, text: *const c_char, encoding: u32) -> CF;
    fn CFStringGetCString(text: CF, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFGetTypeID(value: CF) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFRelease(value: CF);
}
struct Owned(CF);
impl Drop for Owned { fn drop(&mut self) { if !self.0.is_null() { unsafe { CFRelease(self.0) } } } }
pub(super) fn trial_hash() -> Result<String, String> {
    let unavailable = || "HOSTED_DEVICE_UNAVAILABLE".to_string();
    // IOServiceGetMatchingService consumes the matching dictionary; the service
    // and copied property each retain their own release obligation.
    unsafe {
        let matching = IOServiceMatching(c"IOPlatformExpertDevice".as_ptr());
        if matching.is_null() { return Err(unavailable()); }
        let service = IOServiceGetMatchingService(0, matching);
        if service == 0 { return Err(unavailable()); }
        let key = Owned(CFStringCreateWithCString(ptr::null(), c"IOPlatformUUID".as_ptr(), 0x08000100));
        if key.0.is_null() { IOObjectRelease(service); return Err(unavailable()); }
        let property = Owned(IORegistryEntryCreateCFProperty(service, key.0, ptr::null(), 0));
        IOObjectRelease(service);
        if property.0.is_null() || CFGetTypeID(property.0) != CFStringGetTypeID() { return Err(unavailable()); }
        let mut bytes = [0u8; 129];
        if !CFStringGetCString(property.0, bytes.as_mut_ptr().cast(), bytes.len() as isize, 0x08000100) { return Err(unavailable()); }
        let raw = CStr::from_ptr(bytes.as_ptr().cast()).to_str().map_err(|_| unavailable())?;
        let result = super::hash_platform_uuid(raw);
        bytes.fill(0);
        result
    }
}
