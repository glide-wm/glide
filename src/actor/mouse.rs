// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::mem::replace;
use std::rc::Rc;
use std::sync::Arc;

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::{MainThreadMarker, NSInteger};
use tracing::{debug, error, warn};

use super::reactor::{self, Event};
use crate::config::Config;
use crate::sys::event;
use crate::sys::geometry::{CGRectExt, ToICrate};
use crate::sys::screen::CoordinateConverter;
use crate::sys::window_server::{self, WindowServerId, get_window};
use crate::{actor, trace_call};

#[derive(Debug)]
pub enum Request {
    Warp(CGPoint),
    /// The system resets the hidden state of the mouse each time the focused
    /// application changes. WmController sends us this request when that
    /// happens, so we can re-hide the mouse if it is supposed to be hidden.
    EnforceHidden,
    ScreenParametersChanged(Vec<CGRect>, CoordinateConverter),
    ConfigUpdated(Arc<Config>),
}

pub struct Mouse {
    config: RefCell<Arc<Config>>,
    events_tx: reactor::Sender,
    requests_rx: Option<Receiver>,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    hide_count: u32,
    above_window: Option<WindowServerId>,
    above_window_level: NSWindowLevel,
    converter: CoordinateConverter,
    screens: Vec<CGRect>,
}

pub type Sender = actor::Sender<Request>;
pub type Receiver = actor::Receiver<Request>;

impl Mouse {
    pub fn new(config: Arc<Config>, events_tx: reactor::Sender, requests_rx: Receiver) -> Self {
        Mouse {
            config: RefCell::new(config),
            events_tx,
            requests_rx: Some(requests_rx),
            state: RefCell::new(State::default()),
        }
    }

    pub async fn run(mut self) {
        let mut requests_rx = self.requests_rx.take().unwrap();
        let this = Rc::new(self);

        let events = vec![
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGEventType::MouseMoved,
            CGEventType::LeftMouseDragged,
            CGEventType::RightMouseDragged,
            CGEventType::ScrollWheel,
        ];
        let this_ = Rc::clone(&this);
        let current = CFRunLoop::get_current();
        let mtm = MainThreadMarker::new().unwrap();
        // SAFETY: tap is dropped before all captures, and the tap is installed
        // on the current thread.
        let tap = unsafe {
            CGEventTap::new_unchecked(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                events,
                move |_, event_type, event| {
                    this_.on_event(event_type, event, mtm);
                    CallbackResult::Keep
                },
            )
        }
        .expect("Could not create event tap");

        let loop_source = tap.mach_port().create_runloop_source(0).unwrap();
        current.add_source(&loop_source, unsafe { kCFRunLoopCommonModes });

        // Callbacks will be dispatched by the run loop, which we assume is
        // running by the time this function is awaited.
        tap.enable();

        this.apply_config();

        while let Some((_span, request)) = requests_rx.recv().await {
            this.on_request(request);
        }
    }

    fn apply_config(&self) {
        if self.config.borrow().settings.mouse_hides_on_focus {
            if let Err(e) = window_server::allow_hide_mouse() {
                error!(
                    "Could not enable mouse hiding: {e:?}. \
                    mouse_hides_on_focus will have no effect."
                );
            }
        }
    }

    fn on_request(self: &Rc<Self>, request: Request) {
        let mut state = self.state.borrow_mut();
        let config = self.config.borrow();
        match request {
            Request::Warp(point) => {
                if let Err(e) = event::warp_mouse(point) {
                    warn!("Failed to warp mouse: {e:?}");
                }
                if config.settings.mouse_follows_focus
                    && config.settings.mouse_hides_on_focus
                    && state.hide_count == 0
                {
                    debug!("Hiding mouse");
                    state.hide_mouse();
                }
            }
            Request::EnforceHidden => {
                if state.hide_count > 0 {
                    state.hide_mouse();
                }
            }
            Request::ScreenParametersChanged(frames, converter) => {
                state.screens = frames;
                state.converter = converter;
            }
            Request::ConfigUpdated(new_config) => {
                drop(config);
                *self.config.borrow_mut() = new_config;
                self.apply_config();
            }
        }
    }

    fn on_event(self: &Rc<Self>, event_type: CGEventType, event: &CGEvent, mtm: MainThreadMarker) {
        let mut state = self.state.borrow_mut();
        let is_scroll = matches!(event_type, CGEventType::ScrollWheel);
        if !is_scroll && state.hide_count > 0 {
            debug!("Showing mouse");
            state.show_mouse();
        }
        match event_type {
            CGEventType::LeftMouseDown => {
                let loc = event.location();
                self.events_tx.send(Event::LeftMouseDown(loc.to_icrate()));
            }
            CGEventType::LeftMouseUp => {
                self.events_tx.send(Event::MouseUp);
            }
            CGEventType::LeftMouseDragged => {
                let loc = event.location();
                self.events_tx.send(Event::LeftMouseDragged(loc.to_icrate()));
            }
            CGEventType::MouseMoved if self.config.borrow().settings.focus_follows_mouse => {
                let loc = event.location();
                #[cfg(false)]
                tracing::trace!("Mouse moved {loc:?}");
                if let Some(wsid) = state.track_mouse_move(loc.to_icrate(), mtm) {
                    self.events_tx.send(Event::MouseMovedOverWindow(wsid));
                }
            }
            CGEventType::ScrollWheel => {
                let delta_y = event
                    .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_1)
                    as f64;
                let delta_x = event
                    .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_2)
                    as f64;

                if delta_x != 0.0 || delta_y != 0.0 {
                    let alt_held = event.get_flags().contains(CGEventFlags::CGEventFlagAlternate);
                    self.events_tx.send(Event::ScrollWheel { delta_x, delta_y, alt_held });
                }
            }
            _ => (),
        }
    }
}

impl State {
    fn hide_mouse(&mut self) {
        if let Err(e) = event::hide_mouse() {
            warn!("Failed to hide mouse: {e:?}");
        }
        self.hide_count += 1;
    }

    fn show_mouse(&mut self) {
        while self.hide_count > 0 {
            if let Err(e) = event::show_mouse() {
                warn!("Failed to show mouse: {e:?}");
            }
            self.hide_count -= 1;
        }
    }

    fn track_mouse_move(&mut self, loc: CGPoint, mtm: MainThreadMarker) -> Option<WindowServerId> {
        // This takes on the order of 200µs, which can be a while for something
        // that may run many times a second on the main thread. For now this
        // isn't a problem, but when we start doing anything with UI we might
        // want to compute this internally.
        // let new_window = trace_call!(window_server::get_window_at_point(loc, self.converter, mtm));
        let new_window = window_server::get_window_at_point(loc, self.converter, mtm);
        if self.above_window == new_window {
            return None;
        }
        debug!("Mouse is now above window {new_window:?} at {loc:?}");

        // There is a gap between the menu bar and the actual menu pop-ups when
        // a menu is opened. When the mouse goes over this gap, the system
        // reports it to be over whatever window happens to be below the menu
        // bar and behind the pop-up. Ignore anything in this gap so we don't
        // dismiss the pop-up. Strangely, it only seems to happen when the mouse
        // travels down from the menu bar and not when it travels back up.
        // First observed on 13.5.2.
        if self.above_window_level == NSMainMenuWindowLevel {
            const WITHIN: f64 = 1.0;
            for screen in &self.screens {
                // The menu bar is just above the frame of the screen.
                if screen.contains(CGPoint::new(loc.x, loc.y + WITHIN))
                    && loc.y < screen.min().y + WITHIN
                {
                    return None;
                }
            }
        }

        let old_window = replace(&mut self.above_window, new_window);
        let new_window_level = new_window
            .and_then(|id| trace_call!(get_window(id)))
            .map(|info| info.layer as NSWindowLevel)
            .unwrap_or(NSWindowLevel::MIN);
        let old_window_level = replace(&mut self.above_window_level, new_window_level);
        debug!(?old_window, ?old_window_level, ?new_window, ?new_window_level);

        // Don't dismiss popups when the mouse moves off them.
        if old_window_level >= NSPopUpMenuWindowLevel {
            return None;
        }

        // Don't focus windows outside the "normal" range.
        if !(0..NSPopUpMenuWindowLevel).contains(&new_window_level) {
            return None;
        }

        new_window
    }
}

/// https://developer.apple.com/documentation/appkit/nswindowlevel?language=objc
pub type NSWindowLevel = NSInteger;
#[allow(non_upper_case_globals)]
pub const NSMainMenuWindowLevel: NSWindowLevel = 24;
#[allow(non_upper_case_globals)]
pub const NSPopUpMenuWindowLevel: NSWindowLevel = 101;
