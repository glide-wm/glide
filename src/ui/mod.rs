//! UI components for Glide window manager.
//!
//! This module contains reusable UI components that can be used by both
//! examples and the main application.

pub mod group_indicator;

pub use group_indicator::{
    Color, GroupDisplayData, GroupIndicatorNSView, GroupKind, IndicatorConfig,
};
