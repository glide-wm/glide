//! Visual example demonstrating the GroupIndicatorView with segmented bar rendering.
//!
//! This example creates a window with rendered group indicators showing
//! different scenarios using the new segmented bar approach with click handling.

use std::cell::RefCell;
use std::rc::Rc;

use glide_wm::ui::{GroupDisplayData, GroupIndicatorNSView, GroupKind};
use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSFont, NSTextField, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

struct IndicatorDemo {
    indicators: Vec<Rc<RefCell<GroupIndicatorNSView>>>,
}

impl IndicatorDemo {
    fn new() -> Self {
        Self { indicators: Vec::new() }
    }

    fn add_indicator(&mut self, indicator: Rc<RefCell<GroupIndicatorNSView>>) {
        self.indicators.push(indicator);
    }
}

fn main() {
    let mtm = MainThreadMarker::new().unwrap();

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let _demo = create_demo_window(mtm);

    println!("Group Indicators Demo Running");
    println!("Visual features:");
    println!("- Segmented bars with visual separators");
    println!("- Blue highlighted segments for selected items");
    println!("- Different orientations (horizontal/vertical)");
    println!("- Various group sizes");

    app.run();
}

fn create_demo_window(mtm: MainThreadMarker) -> IndicatorDemo {
    let window_rect = NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(800.0, 700.0));

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

    let (content_view, demo) = create_content_view(mtm);
    window.setContentView(Some(&content_view));

    demo
}

fn create_content_view(mtm: MainThreadMarker) -> (Retained<NSView>, IndicatorDemo) {
    let content_view = unsafe {
        let view = NSView::alloc(mtm);
        NSView::initWithFrame(
            view,
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 700.0)),
        )
    };

    let scenarios = create_demo_scenarios();
    let mut y_position = 650.0;
    let mut demo = IndicatorDemo::new();

    for (title, group_data) in scenarios {
        // Add title label
        let label = create_label(&title, y_position, mtm);
        unsafe { content_view.addSubview(&label) };
        y_position -= 35.0;

        // Add indicator view
        let indicator_rect =
            NSRect::new(NSPoint::new(50.0, y_position - 30.0), NSSize::new(700.0, 30.0));

        let mut indicator_view = GroupIndicatorNSView::new(indicator_rect, mtm);
        indicator_view.update(group_data.clone());

        let indicator_rc = Rc::new(RefCell::new(indicator_view));

        // Set up click callback that updates the selection
        let indicator_rc_clone = indicator_rc.clone();
        indicator_rc.borrow_mut().set_click_callback(Rc::new(move |segment_index| {
            println!("✨ Segment {} clicked! Updating selection...", segment_index);
            let mut indicator = indicator_rc_clone.borrow_mut();
            indicator.click_segment(segment_index);
        }));

        unsafe {
            content_view.addSubview(indicator_rc.borrow().view());
        }

        demo.add_indicator(indicator_rc);

        y_position -= 50.0;
    }

    // Add instructions at the bottom
    let instructions = create_label(
        "Interactive Demo: Click on segments to change selection and see animation!",
        y_position - 20.0,
        mtm,
    );
    unsafe { content_view.addSubview(&instructions) };

    (content_view, demo)
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
            "Small horizontal group (3 tabs, middle selected) - Click any segment!",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 3,
                selected_index: 1,
            },
        ),
        (
            "Small vertical group (4 windows, first selected) - Click any segment!",
            GroupDisplayData {
                group_kind: GroupKind::Vertical,
                total_count: 4,
                selected_index: 0,
            },
        ),
        (
            "Medium horizontal group (8 tabs, segment 4 selected) - Click any segment!",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 8,
                selected_index: 4,
            },
        ),
        (
            "Large horizontal group (15 tabs, segment 7 selected) - Click any segment!",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 15,
                selected_index: 7,
            },
        ),
        (
            "Large vertical group (12 windows, segment 3 selected) - Click any segment!",
            GroupDisplayData {
                group_kind: GroupKind::Vertical,
                total_count: 12,
                selected_index: 3,
            },
        ),
        (
            "Single item group (no separators) - Nothing to click!",
            GroupDisplayData {
                group_kind: GroupKind::Horizontal,
                total_count: 1,
                selected_index: 0,
            },
        ),
        (
            "Two item vertical group (one separator) - Click either segment!",
            GroupDisplayData {
                group_kind: GroupKind::Vertical,
                total_count: 2,
                selected_index: 1,
            },
        ),
    ]
}
