// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use objc2::MainThreadMarker;
use tracing::{Span, debug, instrument, warn};

use crate::actor::app::{self, AppThreadHandle, Quiet, WindowId, WindowInfo};
use crate::actor::wm_controller::{self, WmEvent};
use crate::actor::{self, reactor};
use crate::collections::HashMap;
use crate::sys::event::MouseState;
use crate::sys::screen::{NSScreenInfo, ScreenCache};
use crate::sys::window_server::{
    self as sys_ws, SkylightConnection, SkylightNotifier, WindowServerId, get_window,
    kCGSWindowIsInvisible, kCGSWindowIsTerminated, kCGSWindowIsVisible,
};

pub use crate::actor::app::pid_t;

pub struct WindowServer(Rc<RefCell<State>>);

struct State {
    #[expect(unused)]
    mtm: MainThreadMarker,
    connection: SkylightConnection,
    notifiers: Vec<SkylightNotifier>,
    weak_self: Weak<RefCell<Self>>,
    screen_cache: ScreenCache,
    /// Registered windows (for SkyLight destruction tracking).
    registered_windows: HashMap<WindowServerId, (WindowId, AppThreadHandle)>,
    /// Window server IDs currently visible on screen.
    visible_window_ids: Vec<WindowServerId>,
    wm_tx: wm_controller::Sender,
    reactor_tx: reactor::Sender,
}

#[derive(Debug)]
pub enum Request {
    // Sent by the NotificationCenter actor.
    /// Screen configuration changed. Carries NSScreenInfo gathered on the main thread.
    ScreenParametersChanged(Vec<NSScreenInfo>),
    /// The active space changed.
    SpaceChanged,

    // Sent by the App actor.
    /// This is to work around a bug introduced in macOS Sequoia where
    /// kAXUIElementDestroyedNotification is not always sent correctly.
    ///
    /// See https://github.com/glide-wm/glide/issues/10.
    RegisterWindow(WindowServerId, WindowId, AppThreadHandle),
    /// A new window was created.
    WindowCreated(WindowId, WindowInfo, MouseState),
    /// The main window of an application changed.
    ApplicationMainWindowChanged(pid_t, Option<WindowId>, Quiet),
    /// A window was minimized or unminimized.
    WindowVisibilityChanged(WindowId),
}

pub type Sender = actor::Sender<Request>;
pub type Receiver = actor::Receiver<Request>;

impl WindowServer {
    pub fn new(
        mtm: MainThreadMarker,
        wm_tx: wm_controller::Sender,
        reactor_tx: reactor::Sender,
    ) -> Self {
        Self(Rc::new_cyclic(|weak_self: &Weak<RefCell<State>>| {
            let mut state = State {
                mtm,
                connection: SkylightConnection::default_for_thread(),
                notifiers: vec![],
                weak_self: weak_self.clone(),
                screen_cache: ScreenCache::new(),
                registered_windows: HashMap::default(),
                visible_window_ids: vec![],
                wm_tx,
                reactor_tx,
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
                self.registered_windows.insert(wsid, (wid, tx));
                if let Err(e) = self.connection.add_window(wsid) {
                    warn!("Failed to update SkylightNotifier window list: {e}");
                }
            }
            Request::ScreenParametersChanged(ns_screens) => {
                let Some((screens, converter)) = self.screen_cache.update_screen_config(ns_screens)
                else {
                    return;
                };
                let windows = sys_ws::get_visible_windows_with_layer(None);
                self.visible_window_ids = windows.iter().map(|w| w.id).collect();
                let event = WmEvent::ScreenParametersChanged {
                    screens: screens.iter().map(|s| s.id).collect(),
                    frames: screens.iter().map(|s| s.visible_frame).collect(),
                    converter,
                    spaces: self.screen_cache.get_screen_spaces(),
                    scale_factors: screens.iter().map(|s| s.scale_factor).collect(),
                    windows,
                };
                self.send_wm_event(event);
            }
            Request::SpaceChanged => {
                let spaces = self.screen_cache.get_screen_spaces();
                let windows = sys_ws::get_visible_windows_with_layer(None);
                self.visible_window_ids = windows.iter().map(|w| w.id).collect();
                self.send_wm_event(WmEvent::SpaceChanged(spaces, windows));
            }
            Request::WindowCreated(wid, info, mouse_state) => {
                self.update_visible_window_ids();
                let ws_info = info.sys_id.and_then(sys_ws::get_window);
                self.reactor_tx.send(reactor::Event::WindowCreated(
                    wid,
                    info,
                    ws_info,
                    mouse_state,
                ));
            }
            Request::ApplicationMainWindowChanged(pid, wid, quiet) => {
                self.update_visible_window_ids();
                self.reactor_tx
                    .send(reactor::Event::ApplicationMainWindowChanged(pid, wid, quiet));
            }
            Request::WindowVisibilityChanged(_window_id) => {
                self.update_visible_window_ids();
            }
        }
    }

    fn update_visible_window_ids(&mut self) {
        self.visible_window_ids = sys_ws::get_visible_window_ids();
    }

    fn send_wm_event(&self, event: WmEvent) {
        _ = self.wm_tx.send((Span::current().clone(), event));
    }

    fn on_window_destroyed(&mut self, wsid: WindowServerId) {
        debug!("Window destroyed: {wsid:?}");
        self.update_visible_window_ids();
        let Some((wid, tx)) = self.registered_windows.remove(&wsid) else {
            return;
        };
        self.connection.on_window_destroyed(wsid);
        _ = tx.send(app::Request::WindowDestroyed(wid));
    }
}
