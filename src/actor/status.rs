//! Manages the status bar icon.

use std::sync::Arc;

use objc2::MainThreadMarker;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    config::Config,
    sys::{
        menu_bar::StatusIcon,
        screen::{SpaceId, get_active_space_number},
    },
};

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

pub type Sender = UnboundedSender<Event>;
pub type Receiver = UnboundedReceiver<Event>;

impl Status {
    pub fn new(config: Arc<Config>, rx: Receiver, mtm: MainThreadMarker) -> Self {
        Self {
            config,
            rx,
            icon: StatusIcon::new(mtm),
        }
    }

    pub async fn run(mut self) {
        while let Some(event) = self.rx.recv().await {
            self.handle_event(event);
        }
    }

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
