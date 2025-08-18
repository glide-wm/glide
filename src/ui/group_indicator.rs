//! Group indicator UI component for displaying window group information.
//!
//! This module provides a segmented bar for visualizing groups:
//! - Horizontal groups (tabbed): horizontal bar at top
//! - Vertical groups (stacked): vertical bar on right side
//! - Each segment represents one child in the group
//! - Selected segment is highlighted

use std::cell::RefCell;

use objc2::{MainThreadOnly, rc::Retained};
use objc2_app_kit::NSView;
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
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

/// The calculated layout for a segmented bar
#[derive(Debug, Clone)]
pub struct SegmentedBarLayout {
    pub group_kind: GroupKind,
    pub total_count: usize,
    pub selected_index: usize,
    pub total_length: f64,
    pub thickness: f64,
}

/// Core logic component for group indicators
struct GroupIndicatorView {
    config: IndicatorConfig,
    layout: Option<SegmentedBarLayout>,
    group_data: Option<GroupDisplayData>,
}

impl GroupIndicatorView {
    /// Create a new indicator view with default configuration
    fn new() -> Self {
        Self {
            config: IndicatorConfig::default(),
            layout: None,
            group_data: None,
        }
    }

    /// Update the indicator with new group data and view dimensions
    fn update(&mut self, group_data: GroupDisplayData, view_bounds: NSRect) {
        let total_length = match group_data.group_kind {
            GroupKind::Horizontal => view_bounds.size.width,
            GroupKind::Vertical => view_bounds.size.height,
        };

        self.group_data = Some(group_data.clone());
        self.layout = Some(self.calculate_layout(&group_data, total_length));
    }

    /// Clear the indicator (no group to display)
    fn clear(&mut self) {
        self.group_data = None;
        self.layout = None;
    }

    /// Calculate the recommended thickness for the indicator bar
    fn recommended_thickness(&self) -> f64 {
        self.config.bar_thickness
    }

    /// Calculate segmented bar layout with actual dimensions
    fn calculate_layout(
        &self,
        group_data: &GroupDisplayData,
        total_length: f64,
    ) -> SegmentedBarLayout {
        SegmentedBarLayout {
            group_kind: group_data.group_kind,
            total_count: group_data.total_count,
            selected_index: group_data.selected_index,
            total_length,
            thickness: self.config.bar_thickness,
        }
    }
}

/// NSView wrapper for displaying group indicators using CALayer
pub struct GroupIndicatorNSView {
    view: Retained<NSView>,
    indicator: RefCell<GroupIndicatorView>,
}

impl GroupIndicatorNSView {
    /// Create a new indicator view
    pub fn new(frame: NSRect, mtm: MainThreadMarker) -> Self {
        let view = unsafe {
            let view = NSView::alloc(mtm);
            NSView::initWithFrame(view, frame)
        };

        view.setWantsLayer(true);

        Self {
            view,
            indicator: RefCell::new(GroupIndicatorView::new()),
        }
    }

    /// Get the underlying NSView
    pub fn view(&self) -> &NSView {
        &self.view
    }

    /// Update the indicator with new group data
    pub fn update(&self, group_data: GroupDisplayData) {
        let bounds = self.view.bounds();
        self.indicator.borrow_mut().update(group_data, bounds);
        self.redraw();
    }

    /// Clear the indicators
    pub fn clear(&self) {
        self.indicator.borrow_mut().clear();
        self.redraw();
    }

    /// Get the recommended thickness for the indicator area
    pub fn recommended_thickness(&self) -> f64 {
        self.indicator.borrow().recommended_thickness()
    }

    /// Trigger a redraw
    fn redraw(&self) {
        unsafe {
            self.view.setNeedsDisplay(true);
        }

        self.draw_indicators();
    }

    /// Draw the indicators using segmented bar approach
    fn draw_indicators(&self) {
        let indicator = self.indicator.borrow();
        let Some(layout) = indicator.layout.as_ref() else {
            return;
        };
        let config = &indicator.config;

        let bounds = self.view.bounds();

        unsafe {
            if let Some(parent_layer) = self.view.layer() {
                // Remove any existing sublayers
                if let Some(sublayers) = parent_layer.sublayers() {
                    for sublayer in sublayers.iter() {
                        sublayer.removeFromSuperlayer();
                    }
                }

                // Draw the segmented bar
                Self::draw_segmented_bar(&parent_layer, config, layout, bounds);
            }
        }
    }

    /// Draw a segmented bar using CALayer
    fn draw_segmented_bar(
        parent_layer: &CALayer,
        config: &IndicatorConfig,
        layout: &SegmentedBarLayout,
        bounds: NSRect,
    ) {
        let (bar_x, bar_y, bar_width, bar_height) = match layout.group_kind {
            GroupKind::Horizontal => {
                // Horizontal bar at the top
                (
                    0.0,
                    bounds.size.height - layout.thickness,
                    bounds.size.width,
                    layout.thickness,
                )
            }
            GroupKind::Vertical => {
                // Vertical bar on the right side
                (
                    bounds.size.width - layout.thickness,
                    0.0,
                    layout.thickness,
                    bounds.size.height,
                )
            }
        };

        unsafe {
            // Draw background bar
            let background_layer = CALayer::layer();
            background_layer.setFrame(NSRect::new(
                NSPoint::new(bar_x, bar_y),
                NSSize::new(bar_width, bar_height),
            ));

            let bg_color = objc2_app_kit::NSColor::colorWithRed_green_blue_alpha(
                config.unselected_color.r,
                config.unselected_color.g,
                config.unselected_color.b,
                config.unselected_color.a,
            );
            background_layer.setBackgroundColor(Some(&bg_color.CGColor()));

            // Add border
            background_layer.setBorderWidth(config.border_width);
            let border_color = objc2_app_kit::NSColor::colorWithRed_green_blue_alpha(
                config.border_color.r,
                config.border_color.g,
                config.border_color.b,
                config.border_color.a,
            );
            background_layer.setBorderColor(Some(&border_color.CGColor()));

            parent_layer.addSublayer(&background_layer);

            // Draw selected segment
            if layout.total_count > 0 && layout.selected_index < layout.total_count {
                let segment_length = match layout.group_kind {
                    GroupKind::Horizontal => bar_width / layout.total_count as f64,
                    GroupKind::Vertical => bar_height / layout.total_count as f64,
                };

                let (seg_x, seg_y, seg_width, seg_height) = match layout.group_kind {
                    GroupKind::Horizontal => {
                        let seg_start = bar_x + (layout.selected_index as f64 * segment_length);
                        (seg_start, bar_y, segment_length, bar_height)
                    }
                    GroupKind::Vertical => {
                        let seg_start = bar_y + (layout.selected_index as f64 * segment_length);
                        (bar_x, seg_start, bar_width, segment_length)
                    }
                };

                let segment_layer = CALayer::layer();
                segment_layer.setFrame(NSRect::new(
                    NSPoint::new(seg_x, seg_y),
                    NSSize::new(seg_width, seg_height),
                ));

                let selected_color = objc2_app_kit::NSColor::colorWithRed_green_blue_alpha(
                    config.selected_color.r,
                    config.selected_color.g,
                    config.selected_color.b,
                    config.selected_color.a,
                );
                segment_layer.setBackgroundColor(Some(&selected_color.CGColor()));

                parent_layer.addSublayer(&segment_layer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segmented_layout() {
        let mut view = GroupIndicatorView::new();

        let group_data = GroupDisplayData {
            group_kind: GroupKind::Horizontal,
            total_count: 5,
            selected_index: 2,
        };
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 100.0));
        view.update(group_data, bounds);

        let layout = view.layout.as_ref().unwrap();
        assert_eq!(layout.total_count, 5);
        assert_eq!(layout.selected_index, 2);
        assert_eq!(layout.group_kind, GroupKind::Horizontal);
        assert_eq!(layout.thickness, 4.0); // Default thickness
    }
}
