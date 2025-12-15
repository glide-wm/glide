use std::cell::RefCell;
use std::rc::{Rc, Weak};

use objc2::MainThreadMarker;
use tracing::{debug, instrument, warn};

use crate::actor::app::{self, AppThreadHandle, WindowId};
use crate::actor::{self};
use crate::collections::HashMap;
use crate::sys::window_server::{SkylightNotifier, WindowServerId, kCGSWindowIsTerminated};

pub struct WindowServer(Rc<RefCell<State>>);

struct State {
    windows: HashMap<WindowServerId, (WindowId, AppThreadHandle)>,
    notifier: SkylightNotifier,
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
        Self(Rc::new_cyclic(|state: &Weak<RefCell<State>>| {
            let state = state.clone();
            let notifier =
                SkylightNotifier::new_for_event(kCGSWindowIsTerminated, move |event, data| {
                    if event != kCGSWindowIsTerminated {
                        return;
                    }
                    assert_eq!(data.len(), size_of::<WindowServerId>());
                    // SAFETY: We just asserted the correct size.
                    let wsid: WindowServerId = unsafe { *data.as_ptr().cast() };
                    let Some(state) = state.upgrade() else {
                        warn!("could not upgrade state in callback");
                        return;
                    };
                    state.borrow_mut().on_window_destroyed(wsid);
                })
                .expect("Initializing SkylightNotifier");
            RefCell::new(State {
                windows: HashMap::default(),
                notifier,
                mtm,
            })
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
    #[instrument(skip(self))]
    fn on_request(&mut self, request: Request) {
        match request {
            Request::RegisterWindow(wsid, wid, tx) => {
                debug!("Window registered: {wsid:?}");
                self.windows.insert(wsid, (wid, tx));
                if let Err(e) = self.notifier.add_window(wsid) {
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
        self.notifier.on_window_destroyed(wsid);
        _ = tx.send(app::Request::WindowDestroyed(wid));
    }
}
