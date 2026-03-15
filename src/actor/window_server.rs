// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use objc2::MainThreadMarker;
use tracing::{Span, debug, instrument, warn};

use crate::actor::app::{self, AppThreadHandle, WindowId};
use crate::actor::wm_controller::{self, WmEvent};
use crate::actor::{self};
use crate::collections::HashMap;
use crate::sys::screen::{NSScreenInfo, ScreenCache};
use crate::sys::window_server::{
    self as sys_ws, SkylightConnection, SkylightNotifier, WindowServerId, get_window,
    kCGSWindowIsInvisible, kCGSWindowIsTerminated, kCGSWindowIsVisible,
};

pub struct WindowServer(Rc<RefCell<State>>);

struct State {
    windows: HashMap<WindowServerId, (WindowId, AppThreadHandle)>,
    connection: SkylightConnection,
    weak_self: Weak<RefCell<Self>>,
    notifiers: Vec<SkylightNotifier>,
    screen_cache: ScreenCache,
    wm_tx: wm_controller::Sender,
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
    /// Screen configuration changed. Carries NSScreenInfo gathered on the main thread.
    ScreenParametersChanged(Vec<NSScreenInfo>),
    /// The active space changed.
    SpaceChanged,
}

pub type Sender = actor::Sender<Request>;
pub type Receiver = actor::Receiver<Request>;

impl WindowServer {
    pub fn new(mtm: MainThreadMarker, wm_tx: wm_controller::Sender) -> Self {
        Self(Rc::new_cyclic(|weak_self: &Weak<RefCell<State>>| {
            let mut state = State {
                windows: HashMap::default(),
                weak_self: weak_self.clone(),
                connection: SkylightConnection::default_for_thread(),
                notifiers: vec![],
                screen_cache: ScreenCache::new(),
                wm_tx,
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
        self.register_callback(kCGSWindowIsVisible, |_this, wsid| {
            let info = get_window(wsid);
            debug!("kCGSWindowIsVisible: {wsid:?}: {info:?}");
        });
        self.register_callback(kCGSWindowIsInvisible, |_this, wsid| {
            let info = get_window(wsid);
            debug!("kCGSWindowIsInvisible: {wsid:?}: {info:?}");
        });
        // NEXT STEPS:
        // Track which windows are visible per app.
        // When we get an event, notice which windows from an app disappeared
        // and appeared at the same time, get their info using
        // CGCopyWindowListInfo, and if they have the same frame, infer that
        // they are tabs. Send an event.
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
            Request::ScreenParametersChanged(ns_screens) => {
                let Some((screens, converter)) = self.screen_cache.update_screen_config(ns_screens)
                else {
                    return;
                };
                let frames = screens.iter().map(|s| s.visible_frame).collect();
                let ids = screens.iter().map(|s| s.id).collect();
                let scale_factors = screens.iter().map(|s| s.scale_factor).collect();
                let spaces = self.screen_cache.get_screen_spaces();
                let windows = self.get_visible_windows();
                let event = WmEvent::ScreenParametersChanged(
                    frames,
                    ids,
                    converter,
                    spaces,
                    scale_factors,
                    windows,
                );
                self.send_wm_event(event);
            }
            Request::SpaceChanged => {
                let spaces = self.screen_cache.get_screen_spaces();
                let windows = self.get_visible_windows();
                self.send_wm_event(WmEvent::SpaceChanged(spaces, windows));
            }
        }
    }

    fn get_visible_windows(&self) -> Vec<sys_ws::WindowServerInfo> {
        sys_ws::get_visible_windows_with_layer(None)
    }

    fn send_wm_event(&self, event: WmEvent) {
        _ = self.wm_tx.send((Span::current().clone(), event));
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
