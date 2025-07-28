use std::ffi::c_int;

use accessibility::AXUIElement;
use accessibility_sys::{AXError, AXUIElementRef, kAXErrorSuccess, pid_t};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, ItemRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::base::CGError;
use core_graphics::display::{
    CGWindowID, CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListOptionOnScreenOnly,
};
use core_graphics::window::{
    CGWindowListCreateDescriptionFromArray, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowNumber, kCGWindowOwnerPID,
};
use objc2_app_kit::NSWindow;
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::MainThreadMarker;
use serde::{Deserialize, Serialize};

use super::geometry::{CGRectDef, ToICrate};
use super::screen::CoordinateConverter;
use crate::sys::app::ProcessSerialNumber;

/// The window ID used by the window server.
///
/// Obtaining this from AXUIElement uses a private API and is *not* guaranteed.
/// Any functionality depending on this should have a backup plan in case it
/// breaks in the future.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowServerId(pub CGWindowID);

impl WindowServerId {
    pub fn new(id: CGWindowID) -> Self {
        WindowServerId(id)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl Into<u32> for WindowServerId {
    fn into(self) -> u32 {
        self.0
    }
}

impl TryFrom<&AXUIElement> for WindowServerId {
    type Error = accessibility::Error;
    fn try_from(element: &AXUIElement) -> Result<Self, accessibility::Error> {
        let mut id = 0;
        let res = unsafe { _AXUIElementGetWindow(element.as_concrete_TypeRef(), &mut id) };
        if res != kAXErrorSuccess {
            return Err(accessibility::Error::Ax(res));
        }
        Ok(WindowServerId(id))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(unused)]
pub struct WindowServerInfo {
    pub id: WindowServerId,
    pub pid: pid_t,
    pub layer: i32,
    #[serde(with = "CGRectDef")]
    pub frame: CGRect,
}

/// Returns a list of windows visible on the screen, in order starting with the
/// frontmost. Excludes desktop elements.
pub fn get_visible_windows_with_layer(layer: Option<i32>) -> Vec<WindowServerInfo> {
    get_visible_windows_raw()
        .iter()
        .filter_map(|win| make_info(win, layer))
        .collect::<Vec<_>>()
}

/// Returns a list of windows visible on the screen, in order starting with the
/// frontmost.
pub fn get_visible_windows_raw() -> CFArray<CFDictionary<CFString, CFType>> {
    // Note that the ordering is not documented. But
    // NSWindow::windowNumbersWithOptions *is* documented to return the windows
    // in order, so we could always combine their information if the behavior
    // changed.
    unsafe {
        CFArray::wrap_under_get_rule(CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        ))
    }
}

fn make_info(
    win: ItemRef<CFDictionary<CFString, CFType>>,
    layer_filter: Option<i32>,
) -> Option<WindowServerInfo> {
    let layer = get_num(&win, unsafe { kCGWindowLayer })?.try_into().ok()?;
    if !(layer_filter.is_none() || layer_filter == Some(layer)) {
        return None;
    }
    let id = get_num(&win, unsafe { kCGWindowNumber })?;
    let pid = get_num(&win, unsafe { kCGWindowOwnerPID })?;
    let frame: CFDictionary = win.find(unsafe { kCGWindowBounds })?.downcast()?;
    let frame = core_graphics_types::geometry::CGRect::from_dict_representation(&frame)?;
    Some(WindowServerInfo {
        id: WindowServerId(id.try_into().ok()?),
        pid: pid.try_into().ok()?,
        layer,
        frame: frame.to_icrate(),
    })
}

pub fn get_windows(ids: &[WindowServerId]) -> Vec<WindowServerInfo> {
    if ids.is_empty() {
        return Vec::new();
    }
    get_windows_inner(ids).iter().flat_map(|w| make_info(w, None)).collect()
}

pub fn get_window(id: WindowServerId) -> Option<WindowServerInfo> {
    get_windows_inner(&[id]).iter().next().and_then(|w| make_info(w, None))
}

fn get_windows_inner(ids: &[WindowServerId]) -> CFArray<CFDictionary<CFString, CFType>> {
    let array = CFArray::from_copyable(ids);
    unsafe {
        CFArray::wrap_under_create_rule(CGWindowListCreateDescriptionFromArray(
            array.as_concrete_TypeRef(),
        ))
    }
}

fn get_num(dict: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<i64> {
    let item: CFNumber = dict.find(key)?.downcast()?;
    Some(item.to_i64()?)
}

pub fn get_window_at_point(
    point: CGPoint,
    converter: CoordinateConverter,
    mtm: MainThreadMarker,
) -> Option<WindowServerId> {
    let ns_loc = converter.convert_point(point)?;
    let win = unsafe { NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(ns_loc, 0, mtm) };
    Some(WindowServerId(win as u32))
}

unsafe extern "C" {
    fn _AXUIElementGetWindow(elem: AXUIElementRef, wid: *mut CGWindowID) -> AXError;
}

/// Set the key window of the window server and application.
pub fn make_key_window(pid: pid_t, wsid: WindowServerId) -> Result<(), ()> {
    // See https://github.com/Hammerspoon/hammerspoon/issues/370#issuecomment-545545468.
    #[allow(non_upper_case_globals)]
    const kCPSUserGenerated: u32 = 0x200;

    let mut event1 = [0; 0x100];
    event1[0x04] = 0xf8;
    event1[0x08] = 0x01;
    event1[0x3a] = 0x10;
    event1[0x3c..0x3c + 4].copy_from_slice(&wsid.0.to_le_bytes());
    event1[0x20..(0x20 + 0x10)].fill(0xff);

    let mut event2 = event1.clone();
    event2[0x08] = 0x02;

    let psn = ProcessSerialNumber::for_pid(pid)?;
    let check = |err| if err == 0 { Ok(()) } else { Err(()) };
    unsafe {
        check(_SLPSSetFrontProcessWithOptions(&psn, wsid.0, kCPSUserGenerated))?;
        check(SLPSPostEventRecordTo(&psn, event1.as_ptr()))?;
        check(SLPSPostEventRecordTo(&psn, event2.as_ptr()))?;
    }
    Ok(())
}

#[link(name = "SkyLight", kind = "framework")]
unsafe extern "C" {
    fn _SLPSSetFrontProcessWithOptions(
        psn: *const ProcessSerialNumber,
        wid: u32,
        mode: u32,
    ) -> CGError;
    fn SLPSPostEventRecordTo(psn: *const ProcessSerialNumber, bytes: *const u8) -> CGError;
}

/// This must be called to allow hiding the mouse from a background application.
///
/// It relies on a private API, so not guaranteed to continue working, but it is
/// discussed by Apple engineers on developer forums.
pub fn allow_hide_mouse() -> Result<(), CGError> {
    let cid = unsafe { CGSMainConnectionID() };
    let property = CFString::from_static_string("SetsCursorInBackground");
    let err = unsafe {
        CGSSetConnectionProperty(
            cid,
            cid,
            property.as_concrete_TypeRef(),
            CFBoolean::true_value().as_CFTypeRef(),
        )
    };
    if err == 0 { Ok(()) } else { Err(err) }
}

type CGSConnectionID = c_int;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> CGSConnectionID;
    fn CGSSetConnectionProperty(
        cid: CGSConnectionID,
        target_cid: CGSConnectionID,
        key: CFStringRef,
        value: CFTypeRef,
    ) -> CGError;
}
