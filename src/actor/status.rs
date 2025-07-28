//! Manages the status bar icon.

use std::sync::Arc;

use objc2::MainThreadMarker;
use tracing::instrument;

use crate::config::Config;
use crate::sys::menu_bar::StatusIcon;
use crate::sys::screen::{SpaceId, get_active_space_number};
use crate::{actor, trace_call};

#[derive(Debug)]
pub enum Event {
    // Note: These should not be filtered for active (they should all be Some)
    // so we can always show the user the current space id.
    SpaceChanged(Vec<Option<SpaceId>>),
    FocusedScreenChanged,
}

pub struct Status {
    #[expect(unused)]
    config: Arc<Config>,
    rx: Receiver,
    icon: StatusIcon,
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

impl Status {
    pub fn new(config: Arc<Config>, rx: Receiver, mtm: MainThreadMarker) -> Self {
        Self {
            config,
            rx,
            icon: StatusIcon::new(mtm),
        }
    }

    pub async fn run(mut self) {
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
                let label = trace_call!(get_active_space_number())
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                self.icon.set_text(&label);
            }
        }
    }
}
