//! Menu bar icon for displaying the current space ID.

use std::ffi::c_void;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSImage, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_core_foundation::CGSize;
use objc2_foundation::{MainThreadMarker, NSData, NSString};
use tracing::{debug, warn};

/// Manages a menu bar icon that displays the current space ID.
pub struct StatusIcon {
    status_item: Retained<NSStatusItem>,
    mtm: MainThreadMarker,
}

impl StatusIcon {
    /// Creates a new menu bar manager.
    pub fn new(mtm: MainThreadMarker) -> Self {
        let status_item = unsafe {
            let status_bar = NSStatusBar::systemStatusBar();
            let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

            // Create parachute icon
            if let Some(button) = status_item.button(mtm)
                && let Some(parachute_image) = create_parachute_icon()
            {
                button.setImage(Some(&parachute_image));
            }

            status_item
        };

        Self { status_item, mtm }
    }

    /// Sets the text next to the icon.
    pub fn set_text(&mut self, text: &str) {
        let ns_title = NSString::from_str(&text);
        unsafe {
            if let Some(button) = self.status_item.button(self.mtm) {
                button.setTitle(&ns_title);
            } else {
                warn!("Could not get button from status item");
            }
        }
    }
}

impl Drop for StatusIcon {
    fn drop(&mut self) {
        debug!("Removing menu bar icon");
        unsafe {
            let status_bar = NSStatusBar::systemStatusBar();
            status_bar.removeStatusItem(&self.status_item);
        }
    }
}

/// Creates the parachute icon from the SVG file
fn create_parachute_icon() -> Option<Retained<NSImage>> {
    // Load the SVG file
    let svg_data = include_str!("../../site/src/assets/parachute-small.svg");
    let ns_data =
        unsafe { NSData::dataWithBytes_length(svg_data.as_ptr() as *const c_void, svg_data.len()) };

    let Some(image) = NSImage::initWithData(NSImage::alloc(), &ns_data) else {
        return None;
    };

    unsafe {
        // Set the image size to be appropriate for menu bar (16x16 points)
        image.setSize(CGSize { width: 16.0, height: 16.0 });
        // Set as template image so it follows system appearance
        // image.setTemplate(true);
    }

    Some(image)
}
