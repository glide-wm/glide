//! Manages the status bar icon.

use std::sync::Arc;

use objc2::MainThreadMarker;
use tracing::instrument;

use crate::config::Config;
use crate::sys::screen::{SpaceId, get_active_space_number};
use crate::sys::status_bar::StatusIcon;
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
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

impl Status {
    pub fn new(config: Arc<Config>, rx: Receiver, mtm: MainThreadMarker) -> Self {
        let mut this = Self { icon: None, config, rx, mtm };
        this.apply_config();
        this
    }

    fn apply_config(&mut self) {
        let icon = self.icon.take();
        if self.config.settings.experimental.status_icon.enable {
            self.icon = icon.or_else(|| Some(StatusIcon::new(self.mtm)));
        }
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
            Event::SpaceChanged(_) | Event::FocusedScreenChanged => {
                // TODO: Move this off the main thread.
                let Some(icon) = &mut self.icon else { return };
                let label = trace_call!(get_active_space_number())
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                icon.set_text(&label);
            }
            Event::ConfigUpdated(config) => {
                self.config = config;
                self.apply_config();
            }
        }
    }
}
