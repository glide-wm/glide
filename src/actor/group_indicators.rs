//! Manages visual indicators for window groups.
//!
//! This actor runs on the main thread and manages GroupIndicatorNSView instances
//! for each group container in the layout tree. Key integration points:
//!
//! ## Group Detection
//! Groups are detected during layout calculation in `get_sizes()`. Only visible
//! groups (those in the selection path) generate indicators. Groups nested within
//! other groups that aren't selected are filtered out.
//!
//! ## Space Reservation
//! The layout calculation reserves static space for each group based on
//! `config.settings.experimental.group_indicators.bar_thickness`. This space is
//! subtracted from the group's frame before positioning windows.
//!
//! ## Event Flow
//! 1. Reactor detects layout changes during `update_layout()`
//! 2. Reactor compares current groups to cached state
//! 3. If groups changed, sends `GroupsUpdated` event to this actor
//! 4. Actor converts coordinates and creates/updates/removes indicator views
//! 5. When screen config changes, actor receives new `CoordinateConverter`
//! 6. When clicked, indicators send events back to reactor (TODO)
//!
//! ## Configuration
//! Group indicators are controlled by `config.settings.experimental.group_indicators`:
//! - `enable`: Whether to show indicators at all
//! - `bar_thickness`: Height/width of indicator bars in pixels
//! - `horizontal_placement`: Whether horizontal groups show indicators on top or bottom
//! - `vertical_placement`: Whether vertical groups show indicators on left or right
//!
//! ## Performance
//! Simple caching strategy: reactor only sends updates when group state actually
//! changes, avoiding unnecessary UI updates during frequent layout calculations.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use objc2::MainThreadMarker;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use tracing::instrument;

use crate::actor::{self, reactor};
use crate::config::{Config, HorizontalPlacement, VerticalPlacement};
use crate::model::{ContainerKind, NodeId};
use crate::sys::screen::{CoordinateConverter, SpaceId};
use crate::ui::group_indicator::{
    GroupDisplayData, GroupIndicatorNSView, GroupKind, IndicatorConfig,
};

#[derive(Debug, Clone)]
pub struct GroupInfo {
    /// The NodeId of the group container
    pub node_id: NodeId,
    /// The space this group exists on
    pub space_id: SpaceId,
    /// The kind of group (Tabbed/Stacked)
    pub container_kind: ContainerKind,
    /// The frame where the group is positioned
    pub frame: CGRect,
    /// Total number of windows in the group
    pub total_count: usize,
    /// Index of the currently selected window (0-based)
    pub selected_index: usize,
}

#[derive(Debug)]
pub enum Event {
    /// Groups have been updated for a space - full replacement
    GroupsUpdated {
        space_id: SpaceId,
        groups: Vec<GroupInfo>,
    },
    /// Selection changed within a specific group
    GroupSelectionChanged {
        node_id: NodeId,
        selected_index: usize,
    },
    /// Screen configuration changed, update coordinate converter
    ScreenParametersChanged(CoordinateConverter),
}

pub struct GroupIndicators {
    config: Arc<Config>,
    rx: Receiver,
    mtm: MainThreadMarker,
    /// Map from NodeId to the indicator view for that group
    indicators: HashMap<NodeId, GroupIndicatorNSView>,
    /// Sender to communicate back to reactor when indicators are clicked
    reactor_tx: reactor::Sender,
    /// Coordinate converter for translating from Quartz to Cocoa coordinates
    coordinate_converter: CoordinateConverter,
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

impl GroupIndicators {
    pub fn new(
        config: Arc<Config>,
        rx: Receiver,
        mtm: MainThreadMarker,
        reactor_tx: reactor::Sender,
        coordinate_converter: CoordinateConverter,
    ) -> Self {
        Self {
            config,
            rx,
            mtm,
            indicators: HashMap::new(),
            reactor_tx,
            coordinate_converter,
        }
    }

    pub async fn run(mut self) {
        // Check if group indicators are enabled in config
        if !self.is_enabled() {
            return;
        }

        while let Some((span, event)) = self.rx.recv().await {
            let _guard = span.enter();
            self.handle_event(event);
        }
    }

    fn is_enabled(&self) -> bool {
        self.config.settings.experimental.group_indicators.enable
    }

    #[instrument(skip(self))]
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::GroupsUpdated { space_id, groups } => {
                self.handle_groups_updated(space_id, groups);
            }
            Event::GroupSelectionChanged { node_id, selected_index } => {
                self.handle_selection_changed(node_id, selected_index);
            }
            Event::ScreenParametersChanged(converter) => {
                self.handle_screen_parameters_changed(converter);
            }
        }
    }

    fn handle_groups_updated(&mut self, _space_id: SpaceId, groups: Vec<GroupInfo>) {
        // Remove indicators that no longer exist for this space
        let group_nodes: std::collections::HashSet<NodeId> =
            groups.iter().map(|g| g.node_id).collect();
        self.indicators.retain(|&node_id, indicator| {
            // Keep indicators that are still in the new groups list
            // TODO: Also check if they're on the same space when we track that
            if group_nodes.contains(&node_id) {
                true
            } else {
                // Clean up indicator that's no longer needed
                indicator.clear();
                false
            }
        });

        // Update or create indicators for current groups
        for group in groups {
            self.update_or_create_indicator(group);
        }
    }

    fn handle_selection_changed(&mut self, node_id: NodeId, selected_index: usize) {
        if let Some(indicator) = self.indicators.get_mut(&node_id) {
            if let Some(mut group_data) = indicator.group_data() {
                group_data.selected_index = selected_index;
                indicator.update(group_data);
            }
        }
    }

    fn handle_screen_parameters_changed(&mut self, converter: CoordinateConverter) {
        self.coordinate_converter = converter;
        tracing::debug!("Updated coordinate converter for group indicators");
    }

    fn handle_indicator_clicked(&mut self, node_id: NodeId, segment_index: usize) {
        // TODO: Send event to reactor when indicators are clicked
        // For now just log the click
        tracing::debug!(?node_id, segment_index, "Group indicator clicked");
        // self.reactor_tx.send(reactor::Event::GroupIndicatorClicked { node_id, segment_index });
    }

    fn update_or_create_indicator(&mut self, group: GroupInfo) {
        let group_kind = match group.container_kind {
            ContainerKind::Tabbed => GroupKind::Horizontal,
            ContainerKind::Stacked => GroupKind::Vertical,
            _ => {
                tracing::warn!(?group.container_kind, "Unexpected container kind for group");
                return;
            }
        };

        let group_data = GroupDisplayData {
            group_kind,
            total_count: group.total_count,
            selected_index: group.selected_index,
        };

        let node_id = group.node_id;
        let needs_creation = !self.indicators.contains_key(&node_id);

        if needs_creation {
            // Create new indicator
            let mut indicator = GroupIndicatorNSView::new(group.frame, self.mtm);
            indicator.update(group_data);

            // Set up click callback
            indicator.set_click_callback(Rc::new(move |segment_index| {
                // TODO: Send click event back to the actor for handling
                // This will require a way to send events to self from the callback
                tracing::debug!(?node_id, segment_index, "Indicator segment clicked");
            }));

            self.indicators.insert(node_id, indicator);
        } else {
            // Update existing indicator
            if let Some(existing) = self.indicators.get_mut(&node_id) {
                existing.update(group_data);
            }
        }

        // Position indicator (works for both new and existing)
        if let Some(indicator) = self.indicators.get(&node_id) {
            self.position_indicator(indicator, group.frame);
        }
    }

    fn position_indicator(&self, indicator: &GroupIndicatorNSView, group_frame: CGRect) {
        let config = self.indicator_config();

        // Get the group data to determine orientation
        let Some(group_data) = indicator.group_data() else {
            tracing::warn!("Cannot position indicator without group data");
            return;
        };

        // Convert from Quartz coordinates (layout system) to Cocoa coordinates (UI)
        let cocoa_group_frame = match self.coordinate_converter.convert_rect(group_frame) {
            Some(frame) => frame,
            None => {
                tracing::warn!("Failed to convert group frame coordinates");
                return;
            }
        };

        let indicator_frame = Self::calculate_indicator_frame(
            cocoa_group_frame,
            group_data.group_kind,
            config.bar_thickness,
            config.horizontal_placement,
            config.vertical_placement,
        );

        // Update the indicator's frame
        unsafe {
            indicator.view().setFrame(indicator_frame);
        }

        tracing::debug!(
            ?group_frame,
            ?cocoa_group_frame,
            ?indicator_frame,
            "Positioned indicator"
        );
    }

    /// Calculate the indicator frame based on group frame and placement settings.
    /// This is a pure function that can be tested without UI components.
    ///
    /// The group_frame is expected to be in Cocoa coordinates (origin at bottom-left).
    // TODO: We should just pass in the coordinates from the layout calculation.
    fn calculate_indicator_frame(
        group_frame: CGRect,
        group_kind: GroupKind,
        thickness: f64,
        horizontal_placement: HorizontalPlacement,
        vertical_placement: VerticalPlacement,
    ) -> CGRect {
        match group_kind {
            GroupKind::Horizontal => {
                // Tabbed groups - horizontal bar
                match horizontal_placement {
                    HorizontalPlacement::Top => {
                        // Top means higher Y values in Cocoa (toward top of screen)
                        CGRect::new(
                            CGPoint::new(
                                group_frame.origin.x,
                                group_frame.origin.y + group_frame.size.height - thickness,
                            ),
                            CGSize::new(group_frame.size.width, thickness),
                        )
                    }
                    HorizontalPlacement::Bottom => {
                        // Bottom means lower Y values in Cocoa (toward bottom of screen)
                        CGRect::new(
                            group_frame.origin,
                            CGSize::new(group_frame.size.width, thickness),
                        )
                    }
                }
            }
            GroupKind::Vertical => {
                // Stacked groups - vertical bar
                match vertical_placement {
                    VerticalPlacement::Left => CGRect::new(
                        group_frame.origin,
                        CGSize::new(thickness, group_frame.size.height),
                    ),
                    VerticalPlacement::Right => CGRect::new(
                        CGPoint::new(
                            group_frame.origin.x + group_frame.size.width - thickness,
                            group_frame.origin.y,
                        ),
                        CGSize::new(thickness, group_frame.size.height),
                    ),
                }
            }
        }
    }

    fn indicator_config(&self) -> IndicatorConfig {
        IndicatorConfig::from(&self.config.settings.experimental.group_indicators)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_info_fields() {
        // Test that GroupInfo struct has expected fields
        // NodeId construction requires layout tree context, so we'll test this
        // when we have the full integration working
        assert_eq!(ContainerKind::Tabbed.is_group(), true);
        assert_eq!(ContainerKind::Stacked.is_group(), true);
        assert_eq!(ContainerKind::Horizontal.is_group(), false);
    }

    #[test]
    fn test_calculate_indicator_frame() {
        // Test with Cocoa coordinates (origin at bottom-left)
        let group_frame = CGRect::new(CGPoint::new(100.0, 200.0), CGSize::new(400.0, 300.0));
        let thickness = 6.0;

        // Test horizontal (tabbed) group - top placement
        // In Cocoa, "top" means higher Y values (toward top of screen)
        let frame_top = GroupIndicators::calculate_indicator_frame(
            group_frame,
            GroupKind::Horizontal,
            thickness,
            HorizontalPlacement::Top,
            VerticalPlacement::Right,
        );
        assert_eq!(frame_top.origin.x, 100.0);
        assert_eq!(frame_top.origin.y, 200.0 + 300.0 - thickness); // Top edge
        assert_eq!(frame_top.size.width, 400.0);
        assert_eq!(frame_top.size.height, thickness);

        // Test horizontal (tabbed) group - bottom placement
        // In Cocoa, "bottom" means lower Y values (toward bottom of screen)
        let frame_bottom = GroupIndicators::calculate_indicator_frame(
            group_frame,
            GroupKind::Horizontal,
            thickness,
            HorizontalPlacement::Bottom,
            VerticalPlacement::Right,
        );
        assert_eq!(frame_bottom.origin.x, 100.0);
        assert_eq!(frame_bottom.origin.y, 200.0); // Bottom edge (at origin)
        assert_eq!(frame_bottom.size.width, 400.0);
        assert_eq!(frame_bottom.size.height, thickness);

        // Test vertical (stacked) group - left placement
        let frame_left = GroupIndicators::calculate_indicator_frame(
            group_frame,
            GroupKind::Vertical,
            thickness,
            HorizontalPlacement::Top,
            VerticalPlacement::Left,
        );
        assert_eq!(frame_left.origin.x, 100.0);
        assert_eq!(frame_left.origin.y, 200.0);
        assert_eq!(frame_left.size.width, thickness);
        assert_eq!(frame_left.size.height, 300.0);

        // Test vertical (stacked) group - right placement
        let frame_right = GroupIndicators::calculate_indicator_frame(
            group_frame,
            GroupKind::Vertical,
            thickness,
            HorizontalPlacement::Top,
            VerticalPlacement::Right,
        );
        assert_eq!(frame_right.origin.x, 100.0 + 400.0 - thickness); // Right edge
        assert_eq!(frame_right.origin.y, 200.0);
        assert_eq!(frame_right.size.width, thickness);
        assert_eq!(frame_right.size.height, 300.0);
    }
}
