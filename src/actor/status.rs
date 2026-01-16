// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manages the status bar icon.

use std::sync::Arc;

use objc2::MainThreadMarker;
use tracing::instrument;

use crate::actor::reactor;
use crate::config::Config;
use crate::sys::screen::{SpaceId, get_active_space_number};
use crate::ui::status_bar::StatusIcon;
use crate::{actor, trace_call};

#[derive(Debug)]
pub enum Event {
    // Note: These should not be filtered for active (they should all be Some)
    // so we can always show the user the current space id.
    SpaceChanged(Vec<Option<SpaceId>>),
    FocusedScreenChanged,
    ConfigUpdated(Arc<Config>),
}

pub struct Status {
    config: Arc<Config>,
    rx: Receiver,
    icon: Option<StatusIcon>,
    mtm: MainThreadMarker,
    reactor_tx: reactor::Sender,
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

impl Status {
    pub fn new(
        config: Arc<Config>,
        rx: Receiver,
        mtm: MainThreadMarker,
        reactor_tx: reactor::Sender,
    ) -> Self {
        let mut this = Self {
            icon: None,
            config,
            rx,
            mtm,
            reactor_tx,
        };
        this.apply_config();
        this
    }

    fn apply_config(&mut self) {
        let icon = self.icon.take();
        if self.config.settings.experimental.status_icon.enable {
            self.icon = icon.or_else(|| Some(StatusIcon::new(self.mtm, self.reactor_tx.clone())));
        }
        self.update_space();
    }

    pub async fn run(mut self) {
        if self.icon.is_none() {
            return;
        }
        while let Some((span, event)) = self.rx.recv().await {
            let _guard = span.enter();
            self.handle_event(event);
        }
    }

    #[instrument(skip(self))]
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::SpaceChanged(_) | Event::FocusedScreenChanged => self.update_space(),
            Event::ConfigUpdated(config) => {
                self.config = config;
                self.apply_config();
            }
        }
    }

    fn update_space(&mut self) {
        let Some(icon) = &mut self.icon else { return };
        if self.config.settings.experimental.status_icon.space_index {
            // TODO: Move this off the main thread.
            let label = trace_call!(get_active_space_number())
                .map(|n| n.to_string())
                .unwrap_or_default();
            icon.set_text(&label);
        } else {
            icon.set_text("");
        }
    }
}
