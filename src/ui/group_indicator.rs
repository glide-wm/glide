//! Group indicator UI component for displaying window group information.
//!
//! This module provides a segmented bar for visualizing groups:
//! - Horizontal groups (tabbed): horizontal bar at top
//! - Vertical groups (stacked): vertical bar on right side
//! - Each segment represents one child in the group
//! - Selected segment is highlighted

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

/// NSView for displaying group indicators using CALayer
pub struct GroupIndicatorNSView {
    view: Retained<NSView>,
    config: IndicatorConfig,

    // Current state
    group_data: Option<GroupDisplayData>,

    // Persistent layers for animation support
    background_layer: Option<Retained<CALayer>>,
    segment_layers: Vec<Retained<CALayer>>,
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
            config: IndicatorConfig::default(),
            group_data: None,
            background_layer: None,
            segment_layers: Vec::new(),
        }
    }

    /// Get the underlying NSView
    pub fn view(&self) -> &NSView {
        &self.view
    }

    /// Update the indicator with new group data
    pub fn update(&mut self, group_data: GroupDisplayData) {
        self.group_data = Some(group_data);
        self.update_layers();
    }

    /// Clear the indicators
    pub fn clear(&mut self) {
        self.group_data = None;
        self.clear_layers();
    }

    /// Get the recommended thickness for the indicator area
    pub fn recommended_thickness(&self) -> f64 {
        self.config.bar_thickness
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

        self.background_layer = None;
        self.segment_layers.clear();
    }

    /// Update the layer structure to match current group data
    fn update_layers(&mut self) {
        let group_data = match &self.group_data {
            Some(data) => data.clone(),
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

        // Ensure we have the right number of segment layers
        self.ensure_segment_layers(group_data.total_count);

        // Update background layer
        self.update_background_layer(&parent_layer, &group_data, bounds);

        // Update segment layers
        self.update_segment_layers(&parent_layer, &group_data, bounds);
    }

    /// Ensure we have the correct number of segment layers
    fn ensure_segment_layers(&mut self, needed_count: usize) {
        // Remove excess layers
        while self.segment_layers.len() > needed_count {
            if let Some(layer) = self.segment_layers.pop() {
                layer.removeFromSuperlayer();
            }
        }

        // Add missing layers
        while self.segment_layers.len() < needed_count {
            let layer = CALayer::layer();
            self.segment_layers.push(layer);
        }
    }

    /// Update or create the background layer
    fn update_background_layer(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: NSRect,
    ) {
        let (bar_x, bar_y, bar_width, bar_height) = self.calculate_bar_frame(group_data, bounds);

        let background_layer = if let Some(existing) = &self.background_layer {
            existing.clone()
        } else {
            let layer = CALayer::layer();
            parent_layer.addSublayer(&layer);
            self.background_layer = Some(layer.clone());
            layer
        };

        // Update frame
        background_layer.setFrame(NSRect::new(
            NSPoint::new(bar_x, bar_y),
            NSSize::new(bar_width, bar_height),
        ));

        // Update appearance
        let bg_color = self.config.unselected_color.to_nscolor();
        unsafe {
            background_layer.setBackgroundColor(Some(&bg_color.CGColor()));
        }

        background_layer.setBorderWidth(self.config.border_width);
        let border_color = self.config.border_color.to_nscolor();
        unsafe {
            background_layer.setBorderColor(Some(&border_color.CGColor()));
        }
    }

    /// Update all segment layers
    fn update_segment_layers(
        &mut self,
        parent_layer: &CALayer,
        group_data: &GroupDisplayData,
        bounds: NSRect,
    ) {
        let (bar_x, bar_y, bar_width, bar_height) = self.calculate_bar_frame(group_data, bounds);

        if group_data.total_count == 0 {
            return;
        }

        let segment_length = match group_data.group_kind {
            GroupKind::Horizontal => bar_width / group_data.total_count as f64,
            GroupKind::Vertical => bar_height / group_data.total_count as f64,
        };

        for (index, layer) in self.segment_layers.iter().enumerate() {
            if index >= group_data.total_count {
                // Hide excess layers
                layer.setHidden(true);
                continue;
            }

            layer.setHidden(false);

            // Calculate segment frame
            let (seg_x, seg_y, seg_width, seg_height) = match group_data.group_kind {
                GroupKind::Horizontal => {
                    let seg_start = bar_x + (index as f64 * segment_length);
                    (seg_start, bar_y, segment_length, bar_height)
                }
                GroupKind::Vertical => {
                    let seg_start = bar_y + (index as f64 * segment_length);
                    (bar_x, seg_start, bar_width, segment_length)
                }
            };

            layer.setFrame(NSRect::new(
                NSPoint::new(seg_x, seg_y),
                NSSize::new(seg_width, seg_height),
            ));

            // Set color based on selection
            let color = if index == group_data.selected_index {
                self.config.selected_color.to_nscolor()
            } else {
                // Make unselected segments transparent so background shows through
                Color::new(0.0, 0.0, 0.0, 0.0).to_nscolor()
            };
            unsafe {
                layer.setBackgroundColor(Some(&color.CGColor()));
            }

            // Ensure layer is added to parent
            if layer.superlayer().is_none() {
                parent_layer.addSublayer(layer);
            }
        }
    }

    /// Calculate the frame for the indicator bar
    fn calculate_bar_frame(
        &self,
        group_data: &GroupDisplayData,
        bounds: NSRect,
    ) -> (f64, f64, f64, f64) {
        match group_data.group_kind {
            GroupKind::Horizontal => {
                // Horizontal bar at the top
                (
                    0.0,
                    bounds.size.height - self.config.bar_thickness,
                    bounds.size.width,
                    self.config.bar_thickness,
                )
            }
            GroupKind::Vertical => {
                // Vertical bar on the right side
                (
                    bounds.size.width - self.config.bar_thickness,
                    0.0,
                    self.config.bar_thickness,
                    bounds.size.height,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_conversion() {
        let color = Color::blue();
        let _ns_color = color.to_nscolor();
        // Just verify it doesn't crash - hard to test color values in unit tests
    }
}
