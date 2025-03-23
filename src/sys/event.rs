use core_graphics::{
    base::CGError,
    display::{
        kCGNullDirectDisplayID, CGDisplayHideCursor, CGDisplayShowCursor, CGWarpMouseCursorPosition,
    },
};
use icrate::{AppKit::NSEvent, Foundation::CGPoint};
use livesplit_hotkey::{ConsumePreference, Hook};
pub use livesplit_hotkey::{Hotkey, KeyCode, Modifiers};
use serde::{Deserialize, Serialize};
use tracing::info_span;

use super::{geometry::ToCGType, screen::CoordinateConverter};
use crate::actor::{
    reactor::Command,
    wm_controller::{Sender, WmCommand, WmEvent},
};

pub struct HotkeyManager {
    hook: Hook,
    events_tx: Sender,
}

impl HotkeyManager {
    pub fn new(events_tx: Sender) -> Self {
        let hook = Hook::with_consume_preference(ConsumePreference::MustConsume).unwrap();
        HotkeyManager { hook, events_tx }
    }

    pub fn register(&self, modifiers: Modifiers, key_code: KeyCode, cmd: Command) {
        self.register_wm(modifiers, key_code, WmCommand::ReactorCommand(cmd))
    }

    pub fn register_wm(&self, modifiers: Modifiers, key_code: KeyCode, cmd: WmCommand) {
        let events_tx = self.events_tx.clone();
        let mut seq = 0;
        self.hook
            .register(Hotkey { modifiers, key_code }, move || {
                seq += 1;
                let span = info_span!("hotkey::press", ?key_code, ?seq);
                events_tx.send((span, WmEvent::Command(cmd.clone()))).unwrap()
            })
            .unwrap();
    }
}

/// The state of the left mouse button.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
pub enum MouseState {
    Down,
    Up,
}

pub fn get_mouse_state() -> MouseState {
    let left_button = unsafe { NSEvent::pressedMouseButtons() } & 0x1 != 0;
    if left_button {
        MouseState::Down
    } else {
        MouseState::Up
    }
}

pub fn get_mouse_pos(converter: CoordinateConverter) -> Option<CGPoint> {
    let ns_loc = unsafe { NSEvent::mouseLocation() };
    converter.convert_point(ns_loc)
}

pub fn warp_mouse(point: CGPoint) -> Result<(), CGError> {
    cg_result(unsafe { CGWarpMouseCursorPosition(point.to_cgtype()) })
}

/// Hide the mouse. Note that this will have no effect unless
/// [`window_server::allow_hide_mouse`] was called or this application is
/// focused.
pub fn hide_mouse() -> Result<(), CGError> {
    cg_result(unsafe { CGDisplayHideCursor(kCGNullDirectDisplayID) })
}

pub fn show_mouse() -> Result<(), CGError> {
    cg_result(unsafe { CGDisplayShowCursor(kCGNullDirectDisplayID) })
}

fn cg_result(err: CGError) -> Result<(), CGError> {
    if err == 0 {
        Ok(())
    } else {
        Err(err)
    }
}
