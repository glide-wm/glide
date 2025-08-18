//! Group indicator UI component for displaying window group information.
//!
//! This module provides a segmented bar for visualizing groups:
//! - Horizontal groups (tabbed): horizontal bar at top
//! - Vertical groups (stacked): vertical bar on right side
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

/// Configuration for segmented bar appearance
#[derive(Debug, Clone)]
pub struct IndicatorConfig {
    pub bar_thickness: f64,
    pub selected_color: Color,
    pub unselected_color: Color,
    pub border_color: Color,
    pub border_width: f64,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self {
            bar_thickness: 4.0,
            selected_color: Color::blue(),
            unselected_color: Color::light_gray(),
            border_color: Color::gray(),
            border_width: 0.5,
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
}

/// Callback for when a segment is clicked
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
                &state.config,
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
        config: &IndicatorConfig,
    ) -> Option<usize> {
        let bar = calculate_bar_frame_with_thickness(group_data, *bounds, config.bar_thickness);

        // Check if point is within the bar bounds
        if point.x < bar.origin.x
            || point.x >= bar.origin.x + bar.size.width
            || point.y < bar.origin.y
            || point.y >= bar.origin.y + bar.size.height
        {
            return None;
        }

        if group_data.total_count == 0 {
            return None;
        }

        let segment_length = match group_data.group_kind {
            GroupKind::Horizontal => bar.size.width / group_data.total_count as f64,
            GroupKind::Vertical => bar.size.height / group_data.total_count as f64,
        };

        let segment_index = match group_data.group_kind {
            GroupKind::Horizontal => {
                let relative_x = point.x - bar.origin.x;
                (relative_x / segment_length).floor() as usize
            }
            GroupKind::Vertical => {
                let relative_y = point.y - bar.origin.y;
                (relative_y / segment_length).floor() as usize
            }
        };

        if segment_index < group_data.total_count {
            Some(segment_index)
        } else {
            None
        }
    }
}

impl GroupIndicatorNSView {
    /// Create a new indicator view
    pub fn new(frame: CGRect, mtm: MainThreadMarker) -> Self {
        let view =
            ClickableIndicatorView::alloc(mtm).set_ivars(RefCell::new(IndicatorState::default()));
        let view: Retained<_> = unsafe { msg_send![super(view), initWithFrame: frame] };

        view.setWantsLayer(true);

        Self { view }
    }

    /// Get the underlying NSView
    pub fn view(&self) -> &NSView {
        &*self.view
    }

    /// Update the indicator with new group data
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

    /// Clear the indicators
    pub fn clear(&mut self) {
        self.view.ivars().borrow_mut().group_data = None;
        self.clear_layers();
    }

    /// Get the recommended thickness for the indicator area
    pub fn recommended_thickness(&self) -> f64 {
        self.view.ivars().borrow().config.bar_thickness
    }

    /// Set the callback for segment clicks
    pub fn set_click_callback(&mut self, callback: SegmentClickCallback) {
        self.view.ivars().borrow_mut().click_callback = Some(callback);
    }

    /// Get the current group data
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
        let state = self.view.ivars().borrow();
        ClickableIndicatorView::segment_at_point_static(point, group_data, &bounds, &state.config)
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

        // Update background layer
        self.update_background_layer(&parent_layer, &group_data, bounds);

        // Update separator layers
        self.update_separator_layers(&parent_layer, &group_data, bounds);

        // Update selected layer
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

    /// Update or create the background layer
    fn update_background_layer(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: CGRect,
    ) {
        let bar = self.calculate_bar_frame(group_data, bounds);

        let mut state = self.view.ivars().borrow_mut();

        let background_layer = if let Some(existing) = &state.background_layer {
            existing.clone()
        } else {
            let layer = CALayer::layer();
            parent_layer.addSublayer(&layer);
            state.background_layer = Some(layer.clone());
            layer
        };

        // Update frame
        background_layer.setFrame(CGRect::new(
            CGPoint::new(bar.origin.x, bar.origin.y),
            CGSize::new(bar.size.width, bar.size.height),
        ));

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

    /// Update separator layers between segments
    fn update_separator_layers(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: CGRect,
    ) {
        let bar = self.calculate_bar_frame(group_data, bounds);

        if group_data.total_count <= 1 {
            return;
        }

        let segment_length = match group_data.group_kind {
            GroupKind::Horizontal => bar.size.width / group_data.total_count as f64,
            GroupKind::Vertical => bar.size.height / group_data.total_count as f64,
        };

        let state = self.view.ivars().borrow();
        for (index, layer) in state.separator_layers.iter().enumerate() {
            // Calculate separator position (between segments)
            let separator_pos = (index + 1) as f64 * segment_length;

            let (sep_x, sep_y, sep_width, sep_height) = match group_data.group_kind {
                GroupKind::Horizontal => {
                    // Vertical line separators
                    (
                        bar.origin.x + separator_pos - 0.5,
                        bar.origin.y,
                        1.0,
                        bar.size.height,
                    )
                }
                GroupKind::Vertical => {
                    // Horizontal line separators
                    (
                        bar.origin.x,
                        bar.origin.y + separator_pos - 0.5,
                        bar.size.width,
                        1.0,
                    )
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

    /// Update the selected segment layer
    fn update_selected_layer(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: CGRect,
    ) {
        if group_data.total_count == 0 {
            return;
        }

        let (selected_layer, bar_thickness) = {
            let mut state = self.view.ivars().borrow_mut();

            let selected_layer = if let Some(existing) = &state.selected_layer {
                existing.clone()
            } else {
                let layer = CALayer::layer();
                parent_layer.addSublayer(&layer);
                state.selected_layer = Some(layer.clone());
                layer
            };

            let bar_thickness = state.config.bar_thickness;
            (selected_layer, bar_thickness)
        };

        let segment_frame = Self::calculate_segment_frame(
            group_data,
            bounds,
            group_data.selected_index,
            bar_thickness,
        );

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
        bar_thickness: f64,
    ) -> CGRect {
        let bar = calculate_bar_frame_with_thickness(group_data, bounds, bar_thickness);

        let segment_length = match group_data.group_kind {
            GroupKind::Horizontal => bar.size.width / group_data.total_count as f64,
            GroupKind::Vertical => bar.size.height / group_data.total_count as f64,
        };

        let (seg_x, seg_y, seg_width, seg_height) = match group_data.group_kind {
            GroupKind::Horizontal => {
                let seg_start = bar.origin.x + (segment_index as f64 * segment_length);
                (seg_start, bar.origin.y, segment_length, bar.size.height)
            }
            GroupKind::Vertical => {
                let seg_start = bar.origin.y + (segment_index as f64 * segment_length);
                (bar.origin.x, seg_start, bar.size.width, segment_length)
            }
        };

        CGRect::new(CGPoint::new(seg_x, seg_y), CGSize::new(seg_width, seg_height))
    }

    /// Move the selected layer to a new segment (immediate update for now)
    fn animate_selection_change(&self, _from_index: usize, to_index: usize) {
        let (selected_layer, group_data, bounds, bar_thickness) = {
            let state = self.view.ivars().borrow();
            let Some(selected_layer) = state.selected_layer.clone() else {
                return;
            };
            let Some(group_data) = state.group_data.clone() else {
                return;
            };
            let bounds = self.view.bounds();
            let bar_thickness = state.config.bar_thickness;
            (selected_layer, group_data, bounds, bar_thickness)
        };

        let to_frame = Self::calculate_segment_frame(&group_data, bounds, to_index, bar_thickness);

        selected_layer.setFrame(to_frame);
    }

    /// Calculate the frame for the indicator bar
    fn calculate_bar_frame(&self, group_data: &GroupDisplayData, bounds: CGRect) -> CGRect {
        let state = self.view.ivars().borrow();
        calculate_bar_frame_with_thickness(group_data, bounds, state.config.bar_thickness)
    }
}

fn calculate_bar_frame_with_thickness(
    group_data: &GroupDisplayData,
    bounds: CGRect,
    thickness: f64,
) -> CGRect {
    match group_data.group_kind {
        GroupKind::Horizontal => {
            // Horizontal bar at the top
            CGRect::new(
                CGPoint::new(0.0, bounds.size.height - thickness),
                CGSize::new(bounds.size.width, thickness),
            )
        }
        GroupKind::Vertical => {
            // Vertical bar on the right side
            CGRect::new(
                CGPoint::new(bounds.size.width - thickness, 0.0),
                CGSize::new(thickness, bounds.size.height),
            )
        }
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

        // Test outside bar area
        assert_eq!(
            view.segment_at_point(CGPoint::new(50.0, 90.0), &horizontal_group),
            None
        );

        let vertical_group = GroupDisplayData {
            group_kind: GroupKind::Vertical,
            total_count: 4,
            selected_index: 2,
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
        assert_eq!(
            view.segment_at_point(CGPoint::new(290.0, 50.0), &vertical_group),
            None
        );
    }
}
