//! Actors in the window manager.
//!
//! Each actor manages some important resource, like an external application or
//! the layout state. The flow of events between these actors defines the
//! overall behavior of the window manager.

pub mod app;
pub mod layout;
pub mod mouse;
pub mod notification_center;
pub mod raise;
pub mod reactor;
pub mod status;
pub mod wm_controller;
