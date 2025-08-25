//! Group indicator UI component for displaying window group information.
//!
//! This module provides a segmented bar for visualizing groups:
//! - Horizontal groups (tabbed): horizontal bar at top
//! - Vertical groups (stacked): vertical bar on the side
//! - Each segment represents one child in the group
//! - Selected segment is highlighted

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::{DeclaredClass, MainThreadOnly, msg_send};
use objc2_app_kit::{NSEvent, NSView};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::MainThreadMarker;
use objc2_quartz_core::CALayer;

use crate::config::{HorizontalPlacement, VerticalPlacement};
use crate::sys::geometry::CGRectExt;

/// RGBA color representation
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn blue() -> Self {
        Self::new(0.0, 0.5, 1.0, 1.0)
    }

    pub fn light_gray() -> Self {
        Self::new(0.8, 0.8, 0.8, 1.0)
    }

    pub fn gray() -> Self {
        Self::new(0.6, 0.6, 0.6, 1.0)
    }

    /// Convert to NSColor for use with CALayer
    pub fn to_nscolor(&self) -> Retained<objc2_app_kit::NSColor> {
        unsafe {
            objc2_app_kit::NSColor::colorWithRed_green_blue_alpha(self.r, self.g, self.b, self.a)
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndicatorConfig {
    pub selected_color: Color,
    pub unselected_color: Color,
    pub border_color: Color,
    pub border_width: f64,
    pub horizontal_placement: HorizontalPlacement,
    pub vertical_placement: VerticalPlacement,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self {
            selected_color: Color::blue(),
            unselected_color: Color::light_gray(),
            border_color: Color::gray(),
            border_width: 0.5,
            horizontal_placement: HorizontalPlacement::Top,
            vertical_placement: VerticalPlacement::Right,
        }
    }
}

impl From<&crate::config::GroupIndicators> for IndicatorConfig {
    fn from(config: &crate::config::GroupIndicators) -> Self {
        Self {
            selected_color: Color::blue(),
            unselected_color: Color::light_gray(),
            border_color: Color::gray(),
            border_width: 0.5,
            horizontal_placement: config.horizontal_placement,
            vertical_placement: config.vertical_placement,
        }
    }
}

/// Group orientation for determining bar placement
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GroupKind {
    Horizontal, // Tabbed groups - bar at top
    Vertical,   // Stacked groups - bar on right
}

/// High-level data about a group for display
#[derive(Debug, Clone)]
pub struct GroupDisplayData {
    pub group_kind: GroupKind,
    pub total_count: usize,
    pub selected_index: usize,
    pub frame: CGRect,
}

pub type SegmentClickCallback = Rc<dyn Fn(usize)>;

/// Inner state for the indicator view
#[derive(Default)]
pub struct IndicatorState {
    config: IndicatorConfig,
    group_data: Option<GroupDisplayData>,
    background_layer: Option<Retained<CALayer>>,
    separator_layers: Vec<Retained<CALayer>>,
    selected_layer: Option<Retained<CALayer>>,
    click_callback: Option<SegmentClickCallback>,
}

objc2::define_class!(
    #[unsafe(super(NSView))]
    #[ivars = RefCell<IndicatorState>]
    pub struct ClickableIndicatorView;

    impl ClickableIndicatorView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let location = unsafe { self.convertPoint_fromView(event.locationInWindow(), None) };

            let state = self.ivars().borrow();
            let Some(group_data) = &state.group_data else { return };
            let Some(segment_index) = Self::segment_at_point_static(
                location,
                group_data,
                &self.bounds(),
            ) else {
                return
            };
            let Some(callback) = state.click_callback.clone() else { return };

            // Drop the active borrow before invoking the callback.
            drop(state);
            callback(segment_index);
        }
    }
);

/// NSView wrapper for displaying group indicators using CALayer
pub struct GroupIndicatorNSView {
    view: Retained<ClickableIndicatorView>,
}

impl ClickableIndicatorView {
    fn segment_at_point_static(
        point: CGPoint,
        group_data: &GroupDisplayData,
        bounds: &CGRect,
    ) -> Option<usize> {
        if !bounds.contains(point) {
            return None;
        }

        if group_data.total_count == 0 {
            return None;
        }

        let pct = match group_data.group_kind {
            GroupKind::Horizontal => point.x / bounds.size.width,
            GroupKind::Vertical => point.y / bounds.size.height,
        };
        let segment_index = (pct * group_data.total_count as f64).floor() as usize;

        if segment_index < group_data.total_count {
            Some(segment_index)
        } else {
            None
        }
    }
}

impl GroupIndicatorNSView {
    pub fn new(frame: CGRect, mtm: MainThreadMarker) -> Self {
        let view =
            ClickableIndicatorView::alloc(mtm).set_ivars(RefCell::new(IndicatorState::default()));
        let view: Retained<_> = unsafe { msg_send![super(view), initWithFrame: frame] };

        view.setWantsLayer(true);

        Self { view }
    }

    pub fn view(&self) -> &NSView {
        &*self.view
    }

    pub fn update(&mut self, group_data: GroupDisplayData) {
        let old_selected = {
            let state = self.view.ivars().borrow();
            state.group_data.as_ref().map(|d| d.selected_index)
        };

        self.view.ivars().borrow_mut().group_data = Some(group_data.clone());
        self.update_layers();

        // Animate if selection changed
        if let Some(old_index) = old_selected {
            if old_index != group_data.selected_index {
                self.animate_selection_change(old_index, group_data.selected_index);
            }
        }
    }

    pub fn clear(&mut self) {
        self.view.ivars().borrow_mut().group_data = None;
        self.clear_layers();
    }

    pub fn set_click_callback(&mut self, callback: SegmentClickCallback) {
        self.view.ivars().borrow_mut().click_callback = Some(callback);
    }

    pub fn group_data(&self) -> Option<GroupDisplayData> {
        self.view.ivars().borrow().group_data.clone()
    }

    /// Handle a click at the given segment index (for demo purposes)
    pub fn click_segment(&mut self, segment_index: usize) {
        if let Some(group_data) = self.group_data() {
            if segment_index < group_data.total_count {
                let mut updated_data = group_data;
                updated_data.selected_index = segment_index;
                self.update(updated_data);
            }
        }
    }

    /// Handle mouse down events for segment clicking
    pub fn handle_mouse_down(&self, event: &NSEvent) {
        let state = self.view.ivars().borrow();
        let Some(group_data) = &state.group_data else {
            return;
        };

        let Some(callback) = &state.click_callback else {
            return;
        };

        // Convert event location to view coordinates
        let location = unsafe { self.view.convertPoint_fromView(event.locationInWindow(), None) };

        // Determine which segment was clicked
        if let Some(segment_index) = self.segment_at_point(location, group_data) {
            callback(segment_index);
        }
    }

    /// Check if a point is inside this view and return the segment index if clicked
    pub fn check_click(&self, window_point: CGPoint) -> Option<usize> {
        let state = self.view.ivars().borrow();
        let Some(group_data) = &state.group_data else {
            return None;
        };

        // Convert window point to view coordinates
        let view_point = self.view.convertPoint_fromView(window_point, None);

        self.segment_at_point(view_point, group_data)
    }

    /// Determine which segment contains the given point
    pub fn segment_at_point(&self, point: CGPoint, group_data: &GroupDisplayData) -> Option<usize> {
        let bounds = self.view.bounds();
        ClickableIndicatorView::segment_at_point_static(point, group_data, &bounds)
    }

    /// Clear all layers
    fn clear_layers(&mut self) {
        unsafe {
            if let Some(parent_layer) = self.view.layer() {
                if let Some(sublayers) = parent_layer.sublayers() {
                    for sublayer in sublayers.iter() {
                        sublayer.removeFromSuperlayer();
                    }
                }
            }
        }

        let mut state = self.view.ivars().borrow_mut();
        state.background_layer = None;
        state.separator_layers.clear();
        state.selected_layer = None;
    }

    /// Update the layer structure to match current group data
    fn update_layers(&mut self) {
        let group_data = match self.group_data() {
            Some(data) => data,
            None => {
                self.clear_layers();
                return;
            }
        };

        let bounds = self.view.bounds();

        let parent_layer = match unsafe { self.view.layer() } {
            Some(layer) => layer,
            None => return,
        };

        // Ensure we have the right number of separator layers
        self.ensure_separator_layers(group_data.total_count);

        self.update_background_layer(&parent_layer, &group_data, bounds);
        self.update_separator_layers(&parent_layer, &group_data, bounds);
        self.update_selected_layer(&parent_layer, &group_data, bounds);
    }

    /// Ensure we have the correct number of separator layers
    fn ensure_separator_layers(&mut self, total_count: usize) {
        // We need (total_count - 1) separators between segments
        let needed_count = if total_count > 1 { total_count - 1 } else { 0 };

        let mut state = self.view.ivars().borrow_mut();

        // Remove excess layers
        while state.separator_layers.len() > needed_count {
            if let Some(layer) = state.separator_layers.pop() {
                layer.removeFromSuperlayer();
            }
        }

        // Add missing layers
        while state.separator_layers.len() < needed_count {
            let layer = CALayer::layer();
            state.separator_layers.push(layer);
        }
    }

    fn update_background_layer(
        &mut self,
        parent_layer: &CALayer,
        _group_data: &GroupDisplayData,
        bounds: CGRect,
    ) {
        let mut state = self.view.ivars().borrow_mut();

        let background_layer = if let Some(existing) = &state.background_layer {
            existing.clone()
        } else {
            let layer = CALayer::layer();
            parent_layer.addSublayer(&layer);
            state.background_layer = Some(layer.clone());
            layer
        };

        // Use full view bounds for the background layer
        background_layer.setFrame(bounds);

        // Update appearance
        let bg_color = state.config.unselected_color.to_nscolor();
        unsafe {
            background_layer.setBackgroundColor(Some(&bg_color.CGColor()));
        }

        background_layer.setBorderWidth(state.config.border_width);
        let border_color = state.config.border_color.to_nscolor();
        unsafe {
            background_layer.setBorderColor(Some(&border_color.CGColor()));
        }
    }

    fn update_separator_layers(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: CGRect,
    ) {
        if group_data.total_count <= 1 {
            return;
        }

        let segment_length = match group_data.group_kind {
            GroupKind::Horizontal => bounds.size.width / group_data.total_count as f64,
            GroupKind::Vertical => bounds.size.height / group_data.total_count as f64,
        };

        let state = self.view.ivars().borrow();
        for (index, layer) in state.separator_layers.iter().enumerate() {
            // Calculate separator position (between segments)
            let separator_pos = (index + 1) as f64 * segment_length;

            let (sep_x, sep_y, sep_width, sep_height) = match group_data.group_kind {
                GroupKind::Horizontal => {
                    // Vertical line separators
                    (separator_pos - 0.5, 0.0, 1.0, bounds.size.height)
                }
                GroupKind::Vertical => {
                    // Horizontal line separators
                    (0.0, separator_pos - 0.5, bounds.size.width, 1.0)
                }
            };

            layer.setFrame(CGRect::new(
                CGPoint::new(sep_x, sep_y),
                CGSize::new(sep_width, sep_height),
            ));

            // Set separator color
            let separator_color = state.config.border_color.to_nscolor();
            unsafe {
                layer.setBackgroundColor(Some(&separator_color.CGColor()));
            }

            // Ensure layer is added to parent
            if layer.superlayer().is_none() {
                parent_layer.addSublayer(layer);
            }
        }
    }

    fn update_selected_layer(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: CGRect,
    ) {
        if group_data.total_count == 0 {
            return;
        }

        let selected_layer = {
            let mut state = self.view.ivars().borrow_mut();

            if let Some(existing) = &state.selected_layer {
                existing.clone()
            } else {
                let layer = CALayer::layer();
                parent_layer.addSublayer(&layer);
                state.selected_layer = Some(layer.clone());
                layer
            }
        };

        let segment_frame =
            Self::calculate_segment_frame(group_data, bounds, group_data.selected_index);

        selected_layer.setFrame(segment_frame);

        let selected_color = {
            let state = self.view.ivars().borrow();
            state.config.selected_color.to_nscolor()
        };
        unsafe {
            selected_layer.setBackgroundColor(Some(&selected_color.CGColor()));
        }
    }

    fn calculate_segment_frame(
        group_data: &GroupDisplayData,
        bounds: CGRect,
        segment_index: usize,
    ) -> CGRect {
        let segment_length = match group_data.group_kind {
            GroupKind::Horizontal => bounds.size.width / group_data.total_count as f64,
            GroupKind::Vertical => bounds.size.height / group_data.total_count as f64,
        };

        let (seg_x, seg_y, seg_width, seg_height) = match group_data.group_kind {
            GroupKind::Horizontal => {
                let seg_start = segment_index as f64 * segment_length;
                (seg_start, 0.0, segment_length, bounds.size.height)
            }
            GroupKind::Vertical => {
                let seg_start = segment_index as f64 * segment_length;
                (0.0, seg_start, bounds.size.width, segment_length)
            }
        };

        CGRect::new(CGPoint::new(seg_x, seg_y), CGSize::new(seg_width, seg_height))
    }

    fn animate_selection_change(&self, _from_index: usize, to_index: usize) {
        let (selected_layer, group_data, bounds) = {
            let state = self.view.ivars().borrow();
            let Some(selected_layer) = state.selected_layer.clone() else {
                return;
            };
            let Some(group_data) = state.group_data.clone() else {
                return;
            };
            let bounds = self.view.bounds();
            (selected_layer, group_data, bounds)
        };

        let to_frame = Self::calculate_segment_frame(&group_data, bounds, to_index);

        selected_layer.setFrame(to_frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_click_detection() {
        let bounds = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(300.0, 100.0));
        let mtm =
            MainThreadMarker::new().unwrap_or_else(|| unsafe { MainThreadMarker::new_unchecked() });
        let view = GroupIndicatorNSView::new(bounds, mtm);

        let horizontal_group = GroupDisplayData {
            group_kind: GroupKind::Horizontal,
            total_count: 3,
            selected_index: 1,
            frame: bounds,
        };

        // Test horizontal segments (each segment is 100px wide)
        // Segment 0: x 0-100, Segment 1: x 100-200, Segment 2: x 200-300
        assert_eq!(
            view.segment_at_point(CGPoint::new(50.0, 98.0), &horizontal_group),
            Some(0)
        );
        assert_eq!(
            view.segment_at_point(CGPoint::new(150.0, 98.0), &horizontal_group),
            Some(1)
        );
        assert_eq!(
            view.segment_at_point(CGPoint::new(250.0, 98.0), &horizontal_group),
            Some(2)
        );

        // Test outside view bounds
        assert_eq!(
            view.segment_at_point(CGPoint::new(50.0, -10.0), &horizontal_group),
            None
        );

        let vertical_group = GroupDisplayData {
            group_kind: GroupKind::Vertical,
            total_count: 4,
            selected_index: 2,
            frame: bounds,
        };

        // Test vertical segments (each segment is 25px tall)
        // Segment 0: y 0-25, Segment 1: y 25-50, etc.
        assert_eq!(
            view.segment_at_point(CGPoint::new(298.0, 12.0), &vertical_group),
            Some(0)
        );
        assert_eq!(
            view.segment_at_point(CGPoint::new(298.0, 37.0), &vertical_group),
            Some(1)
        );
        assert_eq!(
            view.segment_at_point(CGPoint::new(298.0, 62.0), &vertical_group),
            Some(2)
        );
        assert_eq!(
            view.segment_at_point(CGPoint::new(298.0, 87.0), &vertical_group),
            Some(3)
        );

        // Test outside bar area
        // Test outside view bounds
        assert_eq!(
            view.segment_at_point(CGPoint::new(-10.0, 50.0), &vertical_group),
            None
        );
    }
}
