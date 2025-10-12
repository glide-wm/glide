//! Manages visual indicators for window groups.
//!
//! The layout system calculates indicator frames and state. This actor is
//! responsible for managing the UI components themselves, and forwarding events
//! between them and the reactor.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use objc2::MainThreadMarker;
use tracing::instrument;

use crate::actor::{self, reactor};
use crate::config::Config;
use crate::model::{ContainerKind, GroupInfo, NodeId};
use crate::sys::screen::{CoordinateConverter, SpaceId};
use crate::ui::group_indicator::{GroupDisplayData, GroupIndicatorNSView, GroupKind};

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
    indicators: HashMap<NodeId, GroupIndicatorNSView>,
    #[expect(dead_code)]
    reactor_tx: reactor::Sender,
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
        let group_nodes: std::collections::HashSet<NodeId> =
            groups.iter().map(|g| g.node_id).collect();
        self.indicators.retain(|&node_id, indicator| {
            if group_nodes.contains(&node_id) {
                true
            } else {
                indicator.clear();
                false
            }
        });

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

    fn handle_indicator_clicked(node_id: NodeId, segment_index: usize) {
        tracing::debug!(?node_id, segment_index, "Group indicator clicked");
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
            frame: group.frame,
        };

        let node_id = group.node_id;
        let needs_creation = !self.indicators.contains_key(&node_id);

        if needs_creation {
            let mut indicator = GroupIndicatorNSView::new(group.frame, self.mtm);
            indicator.update(group_data);

            indicator.set_click_callback(Rc::new(move |segment_index| {
                Self::handle_indicator_clicked(node_id, segment_index);
            }));

            // Set initial visibility
            indicator.view().setHidden(!group.visible);

            self.indicators.insert(node_id, indicator);
        } else {
            if let Some(existing) = self.indicators.get_mut(&node_id) {
                existing.update(group_data);
                existing.view().setFrame(group.frame);
                // Update visibility
                existing.view().setHidden(!group.visible);
            }
        }
    }
}
