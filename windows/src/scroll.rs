//! Scrolling a list of the window to its end, which windows-reactor has no
//! call for: through UI Automation, as a screen reader would. The list is
//! found by its automation id and moved by its scroll pattern.
//!
//! The calls wait for the window's thread to answer: never make them from
//! that thread.

use std::mem::ManuallyDrop;

use windows_core::BSTR;
use windows_sys::Win32::System::Com::CLSCTX_INPROC_SERVER;

use crate::bindings::{
    CUIAutomation, CoCreateInstance, IUIAutomation, IUIAutomationScrollPattern, PATTERNID,
    PROPERTYID, TreeScope, UIA_HWND, VARIANT, VARTYPE, VT_BSTR,
};

/// `UIA_AutomationIdPropertyId`.
const AUTOMATION_ID: PROPERTYID = PROPERTYID(30011);
/// `UIA_ScrollPatternId`.
const SCROLL_PATTERN: PATTERNID = PATTERNID(10004);
/// `TreeScope_Descendants`.
const DESCENDANTS: TreeScope = 4;
/// `UIA_ScrollPatternNoScroll`: leaves that direction as it is.
const NO_SCROLL: f64 = -1.0;
/// Within this many heights of the list from its end, it follows what is
/// added at the end.
const NEAR: f64 = 0.5;

/// When to scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    Always,
    /// Only if the list was at its end before what was just added: within
    /// [`NEAR`] of it, since what was added pushed the end down.
    IfNearEnd,
}

/// Whether a list scrolled to `percent` of its range (UI Automation's
/// vertical scroll percent), and showing `view_size` percent of its
/// content, is near its end.
pub fn near_end(percent: f64, view_size: f64) -> bool {
    if !(0.0..=100.0).contains(&percent) || view_size <= 0.0 || view_size >= 100.0 {
        // Nothing to scroll.
        return true;
    }
    // What lies below the view, in heights of the view.
    let below = (100.0 - percent) / 100.0 * (100.0 - view_size) / view_size;
    below <= NEAR
}

/// Scrolls the list with automation id `id`, in the window `window`, to its
/// end. `Ok(false)` if it stayed, as [`Follow::IfNearEnd`] can ask.
pub fn to_end(window: isize, id: &str, follow: Follow) -> windows_core::Result<bool> {
    windows_core::init_mta()?;
    // SAFETY: COM calls on interfaces this thread owns, in the
    // multithreaded apartment; the variant owns its string, freed below.
    unsafe {
        let automation: IUIAutomation = CoCreateInstance(
            &CUIAutomation,
            None::<&windows_core::IUnknown>,
            CLSCTX_INPROC_SERVER,
        )?;
        let root = automation.ElementFromHandle(UIA_HWND(window as *mut core::ffi::c_void))?;
        let mut value = VARIANT::default();
        {
            let inner = &mut *value.Anonymous.Anonymous;
            inner.vt = VARTYPE(VT_BSTR as u16);
            inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(id));
        }
        let condition = automation.CreatePropertyCondition(AUTOMATION_ID, &value);
        ManuallyDrop::drop(&mut (*value.Anonymous.Anonymous).Anonymous.bstrVal);
        let list = root.FindFirst(DESCENDANTS, &condition?)?;
        let scroll: IUIAutomationScrollPattern = list.GetCurrentPatternAs(SCROLL_PATTERN)?;
        if follow == Follow::IfNearEnd
            && !near_end(
                scroll.CurrentVerticalScrollPercent()?,
                scroll.CurrentVerticalViewSize()?,
            )
        {
            return Ok(false);
        }
        scroll.SetScrollPercent(NO_SCROLL, 100.0).ok()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_the_end_is_within_half_a_view() {
        // At the end, or with nothing to scroll.
        assert!(near_end(100.0, 10.0));
        assert!(near_end(-1.0, 100.0));
        assert!(near_end(0.0, 100.0));
        // A tenth of the content shown: 9 views of range, so 5% of it is
        // under half a view, 10% is more.
        assert!(near_end(95.0, 10.0));
        assert!(!near_end(90.0, 10.0));
        // Read from the top of a long thread.
        assert!(!near_end(0.0, 10.0));
    }
}
