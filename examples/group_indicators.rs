//! Visual example demonstrating the GroupIndicatorView with segmented bar rendering.
//!
//! This example creates a window with rendered group indicators showing
//! different scenarios using the new segmented bar approach.

use glide_wm::ui::{GroupDisplayData, GroupIndicatorNSView, GroupKind};
use objc2::{MainThreadOnly, rc::Retained};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSFont, NSTextField, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

fn main() {
    let mtm = MainThreadMarker::new().unwrap();

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    create_demo_window(mtm);

    app.run();
}

fn create_demo_window(mtm: MainThreadMarker) {
    let window_rect = NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(800.0, 600.0));

    let window = unsafe {
        let window = NSWindow::alloc(mtm);
        NSWindow::initWithContentRect_styleMask_backing_defer(
            window,
            window_rect,
            NSWindowStyleMask(15), // Titled, closable, miniaturizable, resizable
            NSBackingStoreType::Buffered,
            false,
        )
    };

    window.setTitle(&NSString::from_str("Group Indicators Visual Demo"));
    window.makeKeyAndOrderFront(None);

    let content_view = create_content_view(mtm);
    window.setContentView(Some(&content_view));
}

fn create_content_view(mtm: MainThreadMarker) -> Retained<NSView> {
    let content_view = unsafe {
        let view = NSView::alloc(mtm);
        NSView::initWithFrame(
            view,
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0)),
        )
    };

    // Skip layer styling for now

    let scenarios = create_demo_scenarios();
    let mut y_position = 1000.0;

    for (title, group_data) in scenarios {
        // Add title label
        let label = create_label(&title, y_position, mtm);
        unsafe { content_view.addSubview(&label) };
        y_position -= 35.0;

        // Add indicator view
        let indicator_rect =
            NSRect::new(NSPoint::new(50.0, y_position - 25.0), NSSize::new(700.0, 25.0));

        let mut indicator_view = GroupIndicatorNSView::new(indicator_rect, mtm);
        indicator_view.update(group_data);

        unsafe {
            content_view.addSubview(indicator_view.view());
        }

        y_position -= 45.0;
    }

    content_view
}

fn create_label(text: &str, y_position: f64, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label_rect = NSRect::new(NSPoint::new(50.0, y_position), NSSize::new(700.0, 25.0));

    let label = unsafe {
        let label = NSTextField::alloc(mtm);
        NSTextField::initWithFrame(label, label_rect)
    };

    unsafe {
        label.setStringValue(&NSString::from_str(text));
        label.setEditable(false);
        label.setSelectable(false);
        label.setBezeled(false);
        label.setDrawsBackground(false);
        label.setFont(Some(&NSFont::systemFontOfSize(14.0)));
    }

    label
}

fn create_demo_scenarios() -> Vec<(&'static str, GroupDisplayData)> {
    vec![
        (
            "Small horizontal group (3 tabs, middle selected)",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 3,
                selected_index: 1,
            },
        ),
        (
            "Small vertical group (4 windows, first selected)",
            GroupDisplayData {
                group_kind: GroupKind::Vertical,
                total_count: 4,
                selected_index: 0,
            },
        ),
        (
            "Medium horizontal group (8 tabs, middle selected)",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 8,
                selected_index: 4,
            },
        ),
        (
            "Large horizontal group (15 tabs, middle selected)",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 15,
                selected_index: 7,
            },
        ),
        (
            "Large vertical group (20 windows, near beginning)",
            GroupDisplayData {
                group_kind: GroupKind::Vertical,
                total_count: 20,
                selected_index: 3,
            },
        ),
        (
            "Single item group",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 1,
                selected_index: 0,
            },
        ),
    ]
}
