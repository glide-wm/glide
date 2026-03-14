// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use objc2::MainThreadMarker;
use tracing::{debug, instrument, warn};

use crate::actor::app::{self, AppThreadHandle, WindowId};
use crate::actor::{self};
use crate::collections::HashMap;
use crate::sys::window_server::{
    SkylightConnection, SkylightNotifier, WindowServerId, kCGSWindowIsTerminated,
};

pub struct WindowServer(Rc<RefCell<State>>);

struct State {
    windows: HashMap<WindowServerId, (WindowId, AppThreadHandle)>,
    connection: SkylightConnection,
    weak_self: Weak<RefCell<Self>>,
    notifiers: Vec<SkylightNotifier>,
    #[expect(unused)]
    mtm: MainThreadMarker,
}

#[derive(Debug)]
pub enum Request {
    /// This is to work around a bug introduced in macOS Sequoia where
    /// kAXUIElementDestroyedNotification is not always sent correctly.
    ///
    /// See https://github.com/glide-wm/glide/issues/10.
    RegisterWindow(WindowServerId, WindowId, AppThreadHandle),
}

pub type Sender = actor::Sender<Request>;
pub type Receiver = actor::Receiver<Request>;

impl WindowServer {
    pub fn new(mtm: MainThreadMarker) -> Self {
        Self(Rc::new_cyclic(|weak_self: &Weak<RefCell<State>>| {
            let mut state = State {
                windows: HashMap::default(),
                weak_self: weak_self.clone(),
                connection: SkylightConnection::default_for_thread(),
                notifiers: vec![],
                mtm,
            };
            state.register_callbacks();
            RefCell::new(state)
        }))
    }

    pub async fn run(self, mut requests_rx: Receiver) {
        while let Some((_span, request)) = requests_rx.recv().await {
            let mut state = self.0.borrow_mut();
            state.on_request(request);
        }
    }
}

impl State {
    fn register_callbacks(&mut self) {
        self.register_callback(kCGSWindowIsTerminated, |this, wsid| {
            this.on_window_destroyed(wsid)
        });
    }

    fn register_callback(&mut self, event: u32, callback: fn(&mut Self, WindowServerId)) {
        let weak_self = self.weak_self.clone();
        let notifier = self
            .connection
            .on_event(event, move |event, data| {
                if event != event {
                    return;
                }
                assert_eq!(data.len(), size_of::<WindowServerId>());
                // SAFETY: We just asserted the correct size.
                let wsid: WindowServerId = unsafe { *data.as_ptr().cast() };
                let Some(state) = weak_self.upgrade() else {
                    warn!("could not upgrade state in callback");
                    return;
                };
                callback(&mut state.borrow_mut(), wsid);
            })
            .expect("Initializing SkylightNotifier");
        self.notifiers.push(notifier);
    }

    #[instrument(skip(self))]
    fn on_request(&mut self, request: Request) {
        match request {
            Request::RegisterWindow(wsid, wid, tx) => {
                debug!("Window registered: {wsid:?}");
                self.windows.insert(wsid, (wid, tx));
                if let Err(e) = self.connection.add_window(wsid) {
                    warn!("Failed to update SkylightNotifier window list: {e}");
                }
            }
        }
    }

    fn on_window_destroyed(&mut self, wsid: WindowServerId) {
        debug!("Window destroyed: {wsid:?}");
        let Some((wid, tx)) = self.windows.remove(&wsid) else {
            return;
        };
        self.connection.on_window_destroyed(wsid);
        _ = tx.send(app::Request::WindowDestroyed(wid));
    }
}
