//! Read-only ordinary-browser inspection. This module intentionally has no
//! navigation, input, tab-selection, profile, page-content, or provider-write
//! capability. It may compare only the currently selected tab of each visible
//! Chrome window with one Router-owned canonical URL.

#[cfg(windows)]
fn chrome_omnibox_value_for_window(
    automation: &windows::Win32::UI::Accessibility::IUIAutomation,
    hwnd: windows::Win32::Foundation::HWND,
) -> Result<Option<String>, String> {
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_EditControlTypeId};

    unsafe {
        let root = automation
            .ElementFromHandle(hwnd)
            .map_err(|_| "NORMAL_BROWSER_UIA_UNAVAILABLE")?;
        if root
            .CurrentClassName()
            .map_err(|_| "NORMAL_BROWSER_UIA_UNAVAILABLE")?
            != "Chrome_WidgetWin_1"
        {
            return Ok(None);
        }
        let condition = automation
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
            .map_err(|_| "NORMAL_BROWSER_UIA_UNAVAILABLE")?;
        let element = match root.FindFirst(TreeScope_Descendants, &condition) {
            Ok(element) => element,
            Err(_) => return Ok(None),
        };
        let value = element
            .GetCurrentPatternAs::<windows::Win32::UI::Accessibility::IUIAutomationValuePattern>(
                windows::Win32::UI::Accessibility::UIA_ValuePatternId,
            )
            .map_err(|_| "NORMAL_BROWSER_OMNIBOX_UNAVAILABLE")?
            .CurrentValue()
            .map_err(|_| "NORMAL_BROWSER_OMNIBOX_UNAVAILABLE")?;
        Ok(Some(value.to_string()))
    }
}

#[cfg(windows)]
fn canonical_url_matches(value: &str, expected: &str) -> bool {
    value == expected
}

/// Reads current omnibox values from visible Chrome windows without bringing a
/// window forward or selecting a tab. A Chrome window exposes only its selected
/// tab's omnibox here; background tabs are intentionally not inferred by title.
#[cfg(windows)]
pub fn any_visible_chrome_window_has_canonical_url(expected: &str) -> Result<bool, String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let automation: IUIAutomation = windows::Win32::System::Com::CoCreateInstance(&CUIAutomation, None, windows::Win32::System::Com::CLSCTX_INPROC_SERVER)
            .map_err(|_| "NORMAL_BROWSER_UIA_UNAVAILABLE")?;
        let mut windows: Vec<HWND> = Vec::new();
        unsafe extern "system" fn collect_visible_window(
            hwnd: HWND,
            data: windows::Win32::Foundation::LPARAM,
        ) -> windows::core::BOOL {
            if IsWindowVisible(hwnd).as_bool() {
                let windows = &mut *(data.0 as *mut Vec<HWND>);
                windows.push(hwnd);
            }
            windows::core::BOOL(1)
        }
        EnumWindows(
            Some(collect_visible_window),
            windows::Win32::Foundation::LPARAM(&mut windows as *mut Vec<HWND> as isize),
        )
        .map_err(|_| "NORMAL_BROWSER_WINDOW_ENUMERATION_UNAVAILABLE")?;
        for hwnd in windows {
            match chrome_omnibox_value_for_window(&automation, hwnd) {
                // Do not retain or expose a non-matching ordinary-browser URL.
                // It is compared only to the one Router-owned canonical URL and
                // immediately dropped when it does not match.
                Ok(Some(value)) if canonical_url_matches(&value, expected) => return Ok(true),
                Ok(None) | Err(_) => {}
                Ok(Some(_)) => {}
            }
        }
        Ok(false)
    }
}

#[cfg(not(windows))]
pub fn any_visible_chrome_window_has_canonical_url(_expected: &str) -> Result<bool, String> { Ok(false) }

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::canonical_url_matches;

    #[cfg(windows)]
    #[test]
    fn canonical_url_comparison_is_exact_not_title_or_prefix_based() {
        let expected = "https://chatgpt.com/c/exact-conversation";
        assert!(canonical_url_matches(expected, expected));
        assert!(!canonical_url_matches("https://chatgpt.com/c/exact-conversation/other", expected));
        assert!(!canonical_url_matches("Exact conversation title", expected));
    }
}
