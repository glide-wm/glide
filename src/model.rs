// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module defines the [`LayoutTree`][layout_tree::LayoutTree] data
//! structure, on which all layout logic is defined.

mod layout_mapping;
mod layout_tree;
mod selection;
mod scroll_constraints;
mod size;
mod tree;
mod window;
pub mod spring;
pub mod scroll_viewport;

pub use layout_mapping::SpaceLayoutMapping;
pub use layout_tree::{LayoutId, LayoutKind, LayoutTree};
pub use size::{ContainerKind, Direction, GroupBarInfo, Orientation};
pub use tree::NodeId;
