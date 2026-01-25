// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Defines the [`LayoutManager`] actor.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

use objc2_core_foundation::{CGRect, CGSize};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::actor::app::{WindowId, pid_t};
use crate::collections::{BTreeExt, BTreeSet, HashMap, HashSet};
use crate::config::Config;
use crate::model::{
    ContainerKind, Direction, LayoutId, LayoutTree, Orientation, SpaceLayoutMapping,
};
use crate::sys::geometry::CGSizeExt;
use crate::sys::screen::SpaceId;

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LayoutCommand {
    NextLayout,
    PrevLayout,
    MoveFocus(#[serde(rename = "direction")] Direction),
    Ascend,
    Descend,
    MoveNode(Direction),
    Split(Orientation),
    Group(Orientation),
    Ungroup,
    ToggleFocusFloating,
    ToggleWindowFloating,
    ToggleFullscreen,
    Resize {
        #[serde(rename = "direction")]
        direction: Direction,
        #[serde(default = "default_resize_percent")]
        percent: f64,
    },
}

fn default_resize_percent() -> f64 {
    5.0
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutEvent {
    /// Used during restoration to make sure we don't retain windows for
    /// terminated apps.
    AppsRunningUpdated(HashSet<pid_t>),
    AppClosed(pid_t),
    /// Updates the set of windows for a given app and space.
    WindowsOnScreenUpdated(SpaceId, pid_t, Vec<(WindowId, LayoutWindowInfo)>),
    WindowAdded(SpaceId, WindowId, LayoutWindowInfo),
    WindowRemoved(WindowId),
    WindowFocused(Vec<SpaceId>, WindowId),
    WindowResized {
        wid: WindowId,
        old_frame: CGRect,
        new_frame: CGRect,
        screens: Vec<(SpaceId, CGRect)>,
    },
    SpaceExposed(SpaceId, CGSize),
    MouseMovedOverWindow {
        over: (SpaceId, WindowId),
        current_main: Option<(SpaceId, WindowId)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutWindowInfo {
    pub bundle_id: Option<String>,
    pub layer: Option<i32>,
    pub is_standard: bool,
    pub is_resizable: bool,
}

#[must_use]
#[derive(Debug, Clone, Default)]
pub struct EventResponse {
    /// Windows to raise quietly. No WindowFocused events will be created for
    /// these.
    pub raise_windows: Vec<WindowId>,
    /// Window to focus. This window will be raised after the windows in
    /// raise_windows and a WindowFocused event will be generated.
    pub focus_window: Option<WindowId>,
}

impl LayoutCommand {
    fn modifies_layout(&self) -> bool {
        use LayoutCommand::*;
        match self {
            MoveNode(_) | Group(_) | Ungroup | Resize { .. } => true,

            NextLayout | PrevLayout | MoveFocus(_) | Ascend | Descend | Split(_)
            | ToggleFocusFloating | ToggleWindowFloating | ToggleFullscreen => false,
        }
    }
}

/// Actor that manages the layouts for each space.
///
/// The LayoutManager is the event-driven layer that sits between the Reactor
/// and the LayoutTree model. This actor receives commands and (cleaned up)
/// events from the Reactor, converts them into LayoutTree operations, and
/// calculates the desired position and size of each window. It also manages
/// floating windows.
///
/// LayoutManager keeps a different layout for each screen size a space is used
/// on. See [`SpaceLayoutInfo`] for more details.
//
// TODO: LayoutManager has too many roles. Consider splitting into a few layers:
//
// * Restoration and new/removed windows/apps.
//   * Convert WindowsOnScreenUpdated events into adds/removes.
// * (Virtual workspaces could go around here.)
// * Floating/tiling split.
// * Tiling layout selection (manual and automatic based on size).
// * Tiling layout-specific commands.
//
// If glide core had a true public API I'd expect it to go after the restoration
// or virtual workspaces layer.
#[derive(Serialize, Deserialize)]
pub struct LayoutManager {
    tree: LayoutTree,
    layout_mapping: HashMap<SpaceId, SpaceLayoutMapping>,
    floating_windows: BTreeSet<WindowId>,
    #[serde(skip)]
    active_floating_windows: HashMap<SpaceId, HashMap<pid_t, HashSet<WindowId>>>,
    #[serde(skip)]
    focused_window: Option<WindowId>,
    /// Last window focused in floating mode.
    // TODO: We should keep a stack for each space.
    last_floating_focus: Option<WindowId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WindowClass {
    Untracked,
    FloatByDefault,
    Regular,
}

fn classify_window(info: &LayoutWindowInfo) -> WindowClass {
    use LayoutWindowInfo as Info;
    match info {
        &Info { layer: Some(layer), .. } if layer != 0 => WindowClass::Untracked,

        // Finder reports a nonstandard window that doesn't actually "exist".
        // In general windows with no layer info are suspect, since it means
        // we couldn't find a corresponding window server window, but we try
        // not to lean on this too much since it depends on a private API.
        Info {
            layer: None,
            is_standard: false,
            bundle_id: Some(bundle_id),
            ..
        } if bundle_id == "com.apple.finder" => WindowClass::Untracked,

        Info { is_standard: false, .. } => WindowClass::FloatByDefault,
        Info { is_resizable: false, .. } => WindowClass::FloatByDefault,

        // Float system preferences windows, since they don't resize horiztonally.
        Info { bundle_id: Some(bundle_id), .. } if bundle_id == "com.apple.systempreferences" => {
            WindowClass::FloatByDefault
        }

        _ => WindowClass::Regular,
    }
}

impl LayoutManager {
    pub fn new() -> Self {
        LayoutManager {
            tree: LayoutTree::new(),
            layout_mapping: Default::default(),
            floating_windows: Default::default(),
            active_floating_windows: Default::default(),
            focused_window: None,
            last_floating_focus: None,
        }
    }

    pub fn debug_tree(&self, space: SpaceId) {
        self.debug_tree_desc(space, "", false);
    }

    pub fn debug_tree_desc(&self, space: SpaceId, desc: &'static str, print: bool) {
        macro_rules! log {
            ($print:expr, $($args:tt)*) => {
                if $print {
                    tracing::warn!($($args)*);
                } else {
                    tracing::debug!($($args)*);
                }
            };
        }
        if let Some(layout) = self.try_layout(space) {
            log!(print, "Tree {desc}\n{}", self.tree.draw_tree(layout).trim());
        } else {
            log!(print, "No layout for space {space:?}");
        }
        if let Some(floating) = self.active_floating_windows.get(&space) {
            let floating = floating.values().flatten().collect::<Vec<_>>();
            log!(print, "Floating {floating:?}");
        }
    }

    pub fn handle_event(&mut self, event: LayoutEvent) -> EventResponse {
        debug!(?event);
        match event {
            LayoutEvent::SpaceExposed(space, size) => {
                self.debug_tree(space);
                self.layout_mapping
                    .entry(space)
                    .or_insert_with(|| SpaceLayoutMapping::new(size, &mut self.tree))
                    .activate_size(size, &mut self.tree);
            }
            LayoutEvent::WindowsOnScreenUpdated(space, pid, windows) => {
                self.debug_tree(space);
                // The windows may already be in the layout if we restored a saved state, so
                // make sure not to duplicate or erase them here.
                let window_map = windows.iter().cloned().collect::<HashMap<_, _>>();
                self.last_floating_focus
                    .take_if(|f| f.pid == pid && !window_map.contains_key(f));
                let layout = self.layout(space);
                let floating_active =
                    self.active_floating_windows.entry(space).or_default().entry(pid).or_default();
                floating_active.clear();
                let mut add_floating = Vec::new();
                let tree_windows = windows
                    .iter()
                    .map(|(wid, _info)| *wid)
                    .filter(|wid| {
                        let floating = self.floating_windows.contains(wid);
                        if floating {
                            floating_active.insert(*wid);
                            return false;
                        }
                        if self.tree.window_node(layout, *wid).is_some() {
                            return true;
                        }
                        match classify_window(window_map.get(wid).unwrap()) {
                            WindowClass::Untracked => false,
                            WindowClass::FloatByDefault => {
                                add_floating.push(*wid);
                                false
                            }
                            WindowClass::Regular => true,
                        }
                    })
                    .collect();
                self.tree.set_windows_for_app(self.layout(space), pid, tree_windows);
                for wid in add_floating {
                    self.add_floating_window(wid, Some(space));
                }
            }
            LayoutEvent::AppsRunningUpdated(hash_set) => {
                self.tree.retain_apps(|pid| hash_set.contains(&pid));
            }
            LayoutEvent::AppClosed(pid) => {
                self.tree.remove_windows_for_app(pid);
                self.floating_windows.remove_all_for_pid(pid);
            }
            LayoutEvent::WindowAdded(space, wid, info) => {
                self.debug_tree(space);
                match classify_window(&info) {
                    WindowClass::FloatByDefault => self.add_floating_window(wid, Some(space)),
                    WindowClass::Regular => {
                        let layout = self.layout(space);
                        self.tree.add_window_after(layout, self.tree.selection(layout), wid);
                    }
                    WindowClass::Untracked => (),
                }
            }
            LayoutEvent::WindowRemoved(wid) => {
                self.tree.remove_window(wid);
                self.floating_windows.remove(&wid);
            }
            LayoutEvent::WindowFocused(spaces, wid) => {
                self.focused_window = Some(wid);
                if self.floating_windows.contains(&wid) {
                    self.last_floating_focus = Some(wid);
                } else {
                    for space in spaces {
                        let layout = self.layout(space);
                        if let Some(node) = self.tree.window_node(layout, wid) {
                            self.tree.select(node);
                        }
                    }
                }
            }
            LayoutEvent::WindowResized {
                wid,
                old_frame,
                new_frame,
                screens,
            } => {
                for (space, screen) in screens {
                    let layout = self.layout(space);
                    let Some(node) = self.tree.window_node(layout, wid) else {
                        continue;
                    };
                    if !screen.size.contains(old_frame.size)
                        || !screen.size.contains(new_frame.size)
                    {
                        // Ignore resizes involving sizes outside the normal
                        // screen bounds. This can happen for instance if the
                        // window becomes fullscreen at the system level.
                        debug!("Ignoring out-of-bounds resize");
                        continue;
                    }
                    if new_frame == screen {
                        // Usually this happens because the user double-clicked
                        // the title bar.
                        self.tree.set_fullscreen(node, true);
                    } else if self.tree.is_fullscreen(node) {
                        // Either the user double-clicked the window to restore
                        // it from full-size, or they are in an interactive
                        // resize. In both cases we should ignore the old_frame
                        // because it does not reflect the layout size in the
                        // tree (fullscreen overrides that). In the interactive
                        // case clearing the fullscreen bit will cause us to
                        // resize the window to our expected restore size, and
                        // the next resize event we see from the user will
                        // correctly use that as the old_frame.
                        self.tree.set_fullscreen(node, false);
                    } else {
                        // n.b.: old_frame should reflect the current size in
                        // the layout tree so it can be accurately updated.
                        self.tree.set_frame_from_resize(node, old_frame, new_frame, screen);
                    }
                }
            }
            LayoutEvent::MouseMovedOverWindow {
                over: (new_space, new_wid),
                current_main,
            } => {
                // We want to allow mouse drag to switch between floating
                // windows and between tiled windows, but not from a floating to
                // a tiled window or vice versa. We also don't allow switching
                // to or from untracked windows.

                // If either window isn't in the layout at all, ignore.
                let Some((cur_space, cur_wid)) = current_main else {
                    return EventResponse::default();
                };
                if self.tree.window_node(self.layout(cur_space), cur_wid).is_none()
                    || self.tree.window_node(self.layout(new_space), new_wid).is_none()
                {
                    return EventResponse::default();
                }

                // Only go from a floating window to other floating windows.
                let cur_floating = self.floating_windows.contains(&cur_wid);
                let new_floating = self.floating_windows.contains(&new_wid);
                if cur_floating != new_floating {
                    return EventResponse::default();
                }

                return EventResponse {
                    raise_windows: vec![],
                    focus_window: Some(new_wid),
                };
            }
        }
        EventResponse::default()
    }

    pub fn handle_command(
        &mut self,
        space: Option<SpaceId>,
        visible_spaces: &[SpaceId],
        command: LayoutCommand,
    ) -> EventResponse {
        if let Some(space) = space {
            let layout = self.layout(space);
            debug!("Tree:\n{}", self.tree.draw_tree(layout).trim());
            debug!(selection = ?self.tree.selection(layout));
        }
        let is_floating = if let Some(focus) = self.focused_window {
            self.floating_windows.contains(&focus)
        } else {
            false
        };
        debug!(?self.floating_windows);
        debug!(?self.focused_window, ?self.last_floating_focus, ?is_floating);

        // ToggleWindowFloating is the only command that works when the space is
        // disabled.
        if let LayoutCommand::ToggleWindowFloating = &command {
            let Some(wid) = self.focused_window else {
                return EventResponse::default();
            };
            if is_floating {
                self.remove_floating_window(wid, space);
                self.last_floating_focus = None;
            } else {
                self.add_floating_window(wid, space);
                self.tree.remove_window(wid);
                self.last_floating_focus = Some(wid);
            }
            return EventResponse::default();
        }

        let Some(space) = space else {
            return EventResponse::default();
        };
        let Some(mapping) = self.layout_mapping.get_mut(&space) else {
            error!(
                ?command, ?self.layout_mapping,
                "Could not find layout mapping for current space");
            return EventResponse::default();
        };
        if command.modifies_layout() {
            mapping.prepare_modify(&mut self.tree);
        }
        let layout = mapping.active_layout();

        if let LayoutCommand::ToggleFocusFloating = &command {
            if is_floating {
                let selection = self.tree.window_at(self.tree.selection(layout));
                let mut raise_windows = self.tree.visible_windows_under(self.tree.root(layout));
                // We need to focus some window to transition into floating
                // mode. If there is no selection, pick a window.
                let focus_window = selection.or_else(|| raise_windows.pop());
                return EventResponse { raise_windows, focus_window };
            } else {
                let floating_windows = self
                    .active_floating_windows
                    .entry(space)
                    .or_default()
                    .values()
                    .flatten()
                    .copied();
                let mut raise_windows: Vec<_> =
                    floating_windows.filter(|&wid| Some(wid) != self.last_floating_focus).collect();
                // We need to focus some window to transition into floating
                // mode. If there is no last floating window, pick one.
                let focus_window = self.last_floating_focus.or_else(|| raise_windows.pop());
                return EventResponse { raise_windows, focus_window };
            }
        }

        // Remaining commands only work for tiling layout.
        if is_floating {
            return EventResponse::default();
        }

        let next_space = |direction| {
            // Pick another space based on the order in visible_spaces.
            if visible_spaces.len() <= 1 {
                return None;
            }
            let idx = visible_spaces.iter().enumerate().find(|(_, s)| **s == space)?.0;
            let idx = match direction {
                Direction::Left | Direction::Up => idx as i32 - 1,
                Direction::Right | Direction::Down => idx as i32 + 1,
            };
            let idx = idx.rem_euclid(visible_spaces.len() as i32);
            Some(visible_spaces[idx as usize])
        };

        match command {
            // Handled above.
            LayoutCommand::ToggleWindowFloating => unreachable!(),
            LayoutCommand::ToggleFocusFloating => unreachable!(),

            LayoutCommand::NextLayout => {
                // FIXME: Update windows in the new layout.
                let layout = mapping.change_layout_index(1);
                if let Some(wid) = self.focused_window
                    && let Some(node) = self.tree.window_node(layout, wid)
                {
                    self.tree.select(node);
                }
                EventResponse::default()
            }
            LayoutCommand::PrevLayout => {
                // FIXME: Update windows in the new layout.
                let layout = mapping.change_layout_index(-1);
                if let Some(wid) = self.focused_window
                    && let Some(node) = self.tree.window_node(layout, wid)
                {
                    self.tree.select(node);
                }
                EventResponse::default()
            }
            LayoutCommand::MoveFocus(direction) => {
                let new_focus =
                    self.tree.traverse(self.tree.selection(layout), direction).or_else(|| {
                        let layout = self.layout(next_space(direction)?);
                        Some(self.tree.selection(layout))
                    });
                let focus_window = new_focus.and_then(|new| self.tree.window_at(new));
                let raise_windows = new_focus
                    .map(|new| self.tree.select_returning_surfaced_windows(new))
                    .unwrap_or_default();
                EventResponse { focus_window, raise_windows }
            }
            LayoutCommand::Ascend => {
                self.tree.ascend_selection(layout);
                EventResponse::default()
            }
            LayoutCommand::Descend => {
                self.tree.descend_selection(layout);
                EventResponse::default()
            }
            LayoutCommand::MoveNode(direction) => {
                let selection = self.tree.selection(layout);
                if !self.tree.move_node(layout, selection, direction) {
                    if let Some(new_space) = next_space(direction) {
                        let new_layout = self.layout(new_space);
                        self.tree.move_node_after(self.tree.selection(new_layout), selection);
                    }
                }
                EventResponse::default()
            }
            LayoutCommand::Split(orientation) => {
                // Don't mark as written yet, since merely splitting doesn't
                // usually have a visible effect.
                let selection = self.tree.selection(layout);
                self.tree.nest_in_container(layout, selection, ContainerKind::from(orientation));
                EventResponse::default()
            }
            LayoutCommand::Group(orientation) => {
                if let Some(parent) = self.tree.selection(layout).parent(self.tree.map()) {
                    self.tree.set_container_kind(parent, ContainerKind::group(orientation));
                }
                EventResponse::default()
            }
            LayoutCommand::Ungroup => {
                if let Some(parent) = self.tree.selection(layout).parent(self.tree.map()) {
                    if self.tree.container_kind(parent).is_group() {
                        self.tree.set_container_kind(
                            parent,
                            self.tree.last_ungrouped_container_kind(parent),
                        )
                    }
                }
                EventResponse::default()
            }
            LayoutCommand::ToggleFullscreen => {
                // We don't consider this a structural change so don't save the
                // layout.
                let node = self.tree.selection(layout);
                if self.tree.toggle_fullscreen(node) {
                    // If we have multiple windows in the newly fullscreen node,
                    // make sure they are on top.
                    let node_windows = node
                        .traverse_preorder(self.tree.map())
                        .flat_map(|n| self.tree.window_at(n))
                        .collect();
                    EventResponse {
                        raise_windows: node_windows,
                        focus_window: None,
                    }
                } else {
                    EventResponse::default()
                }
            }
            LayoutCommand::Resize { direction, percent } => {
                let percent = percent.clamp(-100.0, 100.0);
                let node = self.tree.selection(layout);
                self.tree.resize(node, percent / 100.0, direction);
                EventResponse::default()
            }
        }
    }

    fn add_floating_window(&mut self, wid: WindowId, space: Option<SpaceId>) {
        if let Some(space) = space {
            self.active_floating_windows
                .entry(space)
                .or_default()
                .entry(wid.pid)
                .or_default()
                .insert(wid);
        }
        self.floating_windows.insert(wid);
    }

    fn remove_floating_window(&mut self, wid: WindowId, space: Option<SpaceId>) {
        if let Some(space) = space {
            let layout = self.layout(space);
            let selection = self.tree.selection(layout);
            let node = self.tree.add_window_after(layout, selection, wid);
            self.tree.select(node);
            self.active_floating_windows
                .entry(space)
                .or_default()
                .entry(wid.pid)
                .or_default()
                .remove(&wid);
        }
        self.floating_windows.remove(&wid);
    }

    pub fn calculate_layout(
        &self,
        space: SpaceId,
        screen: CGRect,
        config: &Config,
    ) -> Vec<(WindowId, CGRect)> {
        let layout = self.layout(space);
        //debug!("{}", self.tree.draw_tree(space));
        self.tree.calculate_layout(layout, screen, config)
    }

    pub fn calculate_layout_and_groups(
        &self,
        space: SpaceId,
        screen: CGRect,
        config: &Config,
    ) -> (Vec<(WindowId, CGRect)>, Vec<crate::model::GroupInfo>) {
        let layout = self.layout(space);
        self.tree.calculate_layout_and_groups(layout, screen, config)
    }

    fn try_layout(&self, space: SpaceId) -> Option<LayoutId> {
        self.layout_mapping.get(&space)?.active_layout().into()
    }

    fn layout(&self, space: SpaceId) -> LayoutId {
        self.try_layout(space).unwrap()
    }

    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let mut buf = String::new();
        File::open(path)?.read_to_string(&mut buf)?;
        Ok(ron::from_str(&buf)?)
    }

    pub fn save(&self, path: PathBuf) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(path)?.write_all(self.serialize_to_string().as_bytes())?;
        Ok(())
    }

    pub fn serialize_to_string(&self) -> String {
        ron::ser::to_string(&self).unwrap()
    }

    // This seems a bit messy, but it's simpler and more robust to write some
    // reactor tests as integration tests with this actor.
    #[cfg(test)]
    pub(super) fn selected_window(&mut self, space: SpaceId) -> Option<WindowId> {
        let layout = self.layout(space);
        self.tree.window_at(self.tree.selection(layout))
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::CGPoint;
    use pretty_assertions::assert_eq;
    use test_log::test;

    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> CGRect {
        CGRect::new(CGPoint::new(x as f64, y as f64), CGSize::new(w as f64, h as f64))
    }

    fn make_windows(pid: pid_t, num: u32) -> Vec<(WindowId, LayoutWindowInfo)> {
        (1..=num).map(|idx| (WindowId::new(pid, idx), win_info())).collect()
    }

    fn win_info() -> LayoutWindowInfo {
        LayoutWindowInfo {
            bundle_id: None,
            layer: Some(0),
            is_standard: true,
            is_resizable: true,
        }
    }

    impl LayoutManager {
        fn layout_sorted(&self, space: SpaceId, screen: CGRect) -> Vec<(WindowId, CGRect)> {
            let mut layout = self.calculate_layout(
                space,
                screen,
                &Config::default(), // TODO stop being lazy
            );
            layout.sort_by_key(|(wid, _)| *wid);
            layout
        }
    }

    #[test]
    fn it_maintains_separate_layouts_for_each_screen_size() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;
        let windows = make_windows(pid, 3);

        // Set up the starting layout.
        let screen1 = rect(0, 0, 120, 120);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 1)));
        _ = mgr.handle_command(Some(space), &[space], MoveNode(Direction::Up));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 120, 60)),
                (WindowId::new(pid, 2), rect(0, 60, 60, 60)),
                (WindowId::new(pid, 3), rect(60, 60, 60, 60)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        // Introduce new screen size.
        let screen2 = rect(0, 0, 1200, 1200);
        _ = mgr.handle_event(SpaceExposed(space, screen2.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 1200, 600)),
                (WindowId::new(pid, 2), rect(0, 600, 600, 600)),
                (WindowId::new(pid, 3), rect(600, 600, 600, 600)),
            ],
            mgr.layout_sorted(space, screen2),
            "layout was not correctly scaled to new screen size"
        );

        // Change the layout for the second screen size.
        _ = mgr.handle_command(Some(space), &[space], MoveNode(Direction::Down));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 400, 1200)),
                (WindowId::new(pid, 2), rect(400, 0, 400, 1200)),
                (WindowId::new(pid, 3), rect(800, 0, 400, 1200)),
            ],
            mgr.layout_sorted(space, screen2),
        );

        // Switch back to the first size; the layout should be the same as before.
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 120, 60)),
                (WindowId::new(pid, 2), rect(0, 60, 60, 60)),
                (WindowId::new(pid, 3), rect(60, 60, 60, 60)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        // Switch back to the second size.
        _ = mgr.handle_event(SpaceExposed(space, screen2.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 400, 1200)),
                (WindowId::new(pid, 2), rect(400, 0, 400, 1200)),
                (WindowId::new(pid, 3), rect(800, 0, 400, 1200)),
            ],
            mgr.layout_sorted(space, screen2),
        );
    }

    #[test]
    fn it_culls_unmodified_layouts() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;
        let windows = make_windows(pid, 3);

        // Set up the starting layout but do not modify it.
        let screen1 = rect(0, 0, 120, 120);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 1)));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 40, 120)),
                (WindowId::new(pid, 2), rect(40, 0, 40, 120)),
                (WindowId::new(pid, 3), rect(80, 0, 40, 120)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        // Introduce new screen size.
        let screen2 = rect(0, 0, 1200, 1200);
        _ = mgr.handle_event(SpaceExposed(space, screen2.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 1)));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 400, 1200)),
                (WindowId::new(pid, 2), rect(400, 0, 400, 1200)),
                (WindowId::new(pid, 3), rect(800, 0, 400, 1200)),
            ],
            mgr.layout_sorted(space, screen2),
            "layout was not correctly scaled to new screen size"
        );

        // Change the layout for the second screen size.
        _ = mgr.handle_command(Some(space), &[space], MoveNode(Direction::Up));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 1200, 600)),
                (WindowId::new(pid, 2), rect(0, 600, 600, 600)),
                (WindowId::new(pid, 3), rect(600, 600, 600, 600)),
            ],
            mgr.layout_sorted(space, screen2),
        );

        // Switch back to the first size. We should see a downscaled
        // version of the modified layout.
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 120, 60)),
                (WindowId::new(pid, 2), rect(0, 60, 60, 60)),
                (WindowId::new(pid, 3), rect(60, 60, 60, 60)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        // Switch to a third size. We should see a scaled version of the same.
        let screen3 = rect(0, 0, 12, 12);
        _ = mgr.handle_event(SpaceExposed(space, screen3.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 12, 6)),
                (WindowId::new(pid, 2), rect(0, 6, 6, 6)),
                (WindowId::new(pid, 3), rect(6, 6, 6, 6)),
            ],
            mgr.layout_sorted(space, screen3),
        );

        // Modify the layout.
        _ = mgr.handle_command(Some(space), &[space], MoveNode(Direction::Left));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 6, 12)),
                (WindowId::new(pid, 2), rect(6, 0, 3, 12)),
                (WindowId::new(pid, 3), rect(9, 0, 3, 12)),
            ],
            mgr.layout_sorted(space, screen3),
        );

        // Switch back to the first size. We should see a scaled
        // version of the newly modified layout.
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 60, 120)),
                (WindowId::new(pid, 2), rect(60, 0, 30, 120)),
                (WindowId::new(pid, 3), rect(90, 0, 30, 120)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        // Modify the layout in the first size.
        _ = mgr.handle_command(Some(space), &[space], MoveNode(Direction::Right));

        // Switch back to the second screen size, then the first, then the
        // second again. Since the layout was modified in the second size, the
        // windows should go back to the way they were laid out then.
        _ = mgr.handle_event(SpaceExposed(space, screen2.size));
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(SpaceExposed(space, screen2.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 1200, 600)),
                (WindowId::new(pid, 2), rect(0, 600, 600, 600)),
                (WindowId::new(pid, 3), rect(600, 600, 600, 600)),
            ],
            mgr.layout_sorted(space, screen2),
        );
    }

    #[test]
    fn floating_windows() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;
        let config = &Config::default();

        let screen1 = rect(0, 0, 120, 120);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, make_windows(pid, 3)));

        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 2)));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 1)));

        // Make the first window float.
        _ = mgr.handle_command(Some(space), &[space], ToggleWindowFloating);
        let sizes: HashMap<_, _> =
            mgr.calculate_layout(space, screen1, config).into_iter().collect();
        assert_eq!(sizes[&WindowId::new(pid, 2)], rect(0, 0, 60, 120));
        assert_eq!(sizes[&WindowId::new(pid, 3)], rect(60, 0, 60, 120));

        // Toggle back to the tiled windows.
        let response = mgr.handle_command(Some(space), &[space], ToggleFocusFloating);
        assert_eq!(
            vec![WindowId::new(pid, 3), WindowId::new(pid, 2)],
            response.raise_windows
        );
        assert_eq!(Some(WindowId::new(pid, 2)), response.focus_window);
        if let Some(focus) = response.focus_window {
            _ = mgr.handle_event(WindowFocused(vec![space], focus));
        }

        // Make the second window float.
        _ = mgr.handle_command(Some(space), &[space], ToggleWindowFloating);
        let sizes: HashMap<_, _> =
            mgr.calculate_layout(space, screen1, config).into_iter().collect();
        assert_eq!(sizes[&WindowId::new(pid, 3)], rect(0, 0, 120, 120));

        // Toggle back to tiled.
        let response = mgr.handle_command(Some(space), &[space], ToggleFocusFloating);
        assert_eq!(vec![WindowId::new(pid, 3)], response.raise_windows);
        assert_eq!(Some(WindowId::new(pid, 3)), response.focus_window);
        if let Some(focus) = response.focus_window {
            _ = mgr.handle_event(WindowFocused(vec![space], focus));
        }

        // Toggle back to floating.
        let response = mgr.handle_command(Some(space), &[space], ToggleFocusFloating);
        assert_eq!(vec![WindowId::new(pid, 1)], response.raise_windows);
        assert_eq!(Some(WindowId::new(pid, 2)), response.focus_window);
        if let Some(focus) = response.focus_window {
            _ = mgr.handle_event(WindowFocused(vec![space], focus));
        }
    }

    #[test]
    fn floating_windows_space_disabled() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;
        let config = &Config::default();

        _ = mgr.handle_event(WindowFocused(vec![], WindowId::new(pid, 1)));

        // Make the first window float.
        _ = mgr.handle_command(None, &[], ToggleWindowFloating);

        // Enable the space.
        let screen1 = rect(0, 0, 120, 120);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, make_windows(pid, 3)));

        let sizes: HashMap<_, _> =
            mgr.calculate_layout(space, screen1, config).into_iter().collect();
        assert_eq!(sizes[&WindowId::new(pid, 2)], rect(0, 0, 60, 120));
        assert_eq!(sizes[&WindowId::new(pid, 3)], rect(60, 0, 60, 120));

        // Toggle back to the tiled windows.
        let response = mgr.handle_command(Some(space), &[space], ToggleFocusFloating);
        let mut raised_windows = response.raise_windows;
        raised_windows.extend(response.focus_window);
        raised_windows.sort();
        assert_eq!(
            vec![WindowId::new(pid, 2), WindowId::new(pid, 3)],
            raised_windows
        );
        // This if let is kind of load bearing for this test: previously we
        // allowed passing None for the window id of this event, except we
        // did that in the test but not in production. This led to an uncaught
        // bug!
        if let Some(focus) = response.focus_window {
            _ = mgr.handle_event(WindowFocused(vec![space], focus));
        }

        // Toggle back to floating.
        let response = mgr.handle_command(Some(space), &[space], ToggleFocusFloating);
        assert!(response.raise_windows.is_empty());
        assert_eq!(Some(WindowId::new(pid, 1)), response.focus_window);
        if let Some(focus) = response.focus_window {
            _ = mgr.handle_event(WindowFocused(vec![space], focus));
        }
    }

    #[test]
    fn it_adds_new_windows_behind_selection() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;
        let windows = make_windows(pid, 5);

        let screen1 = rect(0, 0, 300, 30);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows.clone()));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 5)));
        _ = mgr.handle_command(Some(space), &[space], ToggleWindowFloating);
        _ = mgr.handle_command(Some(space), &[space], ToggleFocusFloating);
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 2)));
        _ = mgr.handle_command(Some(space), &[space], Split(Orientation::Vertical));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 3)));
        _ = mgr.handle_command(Some(space), &[space], MoveNode(Direction::Left));

        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 15)),
                (WindowId::new(pid, 3), rect(100, 15, 100, 15)),
                (WindowId::new(pid, 4), rect(200, 0, 100, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        // Add a new window when the left window is selected.
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 1)));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 6), win_info()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 75, 30)),
                (WindowId::new(pid, 2), rect(150, 0, 75, 15)),
                (WindowId::new(pid, 3), rect(150, 15, 75, 15)),
                (WindowId::new(pid, 4), rect(225, 0, 75, 30)),
                (WindowId::new(pid, 6), rect(75, 0, 75, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );
        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 6)));

        // Add a new window when the top middle is selected.
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 2)));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 6), win_info()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 10)),
                (WindowId::new(pid, 3), rect(100, 20, 100, 10)),
                (WindowId::new(pid, 4), rect(200, 0, 100, 30)),
                (WindowId::new(pid, 6), rect(100, 10, 100, 10)),
            ],
            mgr.layout_sorted(space, screen1),
        );
        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 6)));

        // Same thing, but unfloat an existing window instead of making a new one.
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 2)));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 5)));
        _ = mgr.handle_command(Some(space), &[space], ToggleWindowFloating);
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 10)),
                (WindowId::new(pid, 3), rect(100, 20, 100, 10)),
                (WindowId::new(pid, 4), rect(200, 0, 100, 30)),
                (WindowId::new(pid, 5), rect(100, 10, 100, 10)),
            ],
            mgr.layout_sorted(space, screen1),
        );
        _ = mgr.handle_command(Some(space), &[space], ToggleWindowFloating);

        // Add a new window when the bottom middle is selected.
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 3)));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 6), win_info()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 10)),
                (WindowId::new(pid, 3), rect(100, 10, 100, 10)),
                (WindowId::new(pid, 4), rect(200, 0, 100, 30)),
                (WindowId::new(pid, 6), rect(100, 20, 100, 10)),
            ],
            mgr.layout_sorted(space, screen1),
        );
        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 6)));

        // Add a new window when the right window is selected.
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 4)));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 6), win_info()));
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 75, 30)),
                (WindowId::new(pid, 2), rect(75, 0, 75, 15)),
                (WindowId::new(pid, 3), rect(75, 15, 75, 15)),
                (WindowId::new(pid, 4), rect(150, 0, 75, 30)),
                (WindowId::new(pid, 6), rect(225, 0, 75, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );
        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 6)));
    }

    #[test]
    fn add_remove_add() {
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;

        let screen1 = rect(0, 0, 300, 30);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, vec![]));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 1), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 2), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 3), win_info()));

        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 30)),
                (WindowId::new(pid, 3), rect(200, 0, 100, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 3)));
        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 1)));
        _ = mgr.handle_event(WindowRemoved(WindowId::new(pid, 2)));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 1), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 2), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 3), win_info()));

        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 30)),
                (WindowId::new(pid, 3), rect(200, 0, 100, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );
    }

    #[test]
    fn resize_to_full_screen_and_back_preserves_layout() {
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;

        let screen1 = rect(0, 0, 300, 30);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, vec![]));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 1), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 2), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 3), win_info()));

        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 30)),
                (WindowId::new(pid, 3), rect(200, 0, 100, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        _ = mgr.handle_event(WindowResized {
            wid: WindowId::new(pid, 2),
            old_frame: rect(100, 0, 100, 30),
            new_frame: screen1,
            screens: vec![(space, screen1)],
        });

        // Check that the other windows aren't resized (especially to zero);
        // otherwise, we will lose the layout state as we receive nonconforming
        // frame changed events.
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), screen1),
                (WindowId::new(pid, 3), rect(200, 0, 100, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );

        _ = mgr.handle_event(WindowResized {
            wid: WindowId::new(pid, 2),
            old_frame: screen1,
            new_frame: rect(100, 0, 100, 30),
            screens: vec![(space, screen1)],
        });

        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 100, 30)),
                (WindowId::new(pid, 2), rect(100, 0, 100, 30)),
                (WindowId::new(pid, 3), rect(200, 0, 100, 30)),
            ],
            mgr.layout_sorted(space, screen1),
        );
    }

    #[test]
    fn resize_to_system_full_screen_and_back_preserves_layout() {
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;

        let screen1 = rect(0, 10, 300, 20);
        let screen1_full = rect(0, 0, 300, 30);
        _ = mgr.handle_event(SpaceExposed(space, screen1.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, vec![]));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 1), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 2), win_info()));
        _ = mgr.handle_event(WindowAdded(space, WindowId::new(pid, 3), win_info()));

        let orig = vec![
            (WindowId::new(pid, 1), rect(0, 10, 100, 20)),
            (WindowId::new(pid, 2), rect(100, 10, 100, 20)),
            (WindowId::new(pid, 3), rect(200, 10, 100, 20)),
        ];
        assert_eq!(orig, mgr.layout_sorted(space, screen1));

        // Simulate a window going fullscreen on the current space.
        //
        // The leftmost window is better for testing because it passes the
        // "only resize in 2 directions" requirement.
        _ = mgr.handle_event(WindowResized {
            wid: WindowId::new(pid, 1),
            old_frame: rect(0, 10, 100, 20),
            new_frame: screen1_full,
            screens: vec![(space, screen1)],
        });

        _ = mgr.handle_event(WindowResized {
            wid: WindowId::new(pid, 1),
            old_frame: screen1_full,
            new_frame: rect(0, 10, 100, 20),
            screens: vec![(space, screen1)],
        });

        assert_eq!(orig, mgr.layout_sorted(space, screen1));
    }

    #[test]
    fn flip_between_screens() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space1 = SpaceId::new(1);
        let space2 = SpaceId::new(2);
        let pid = 1;

        let screen1 = rect(0, 0, 300, 30);
        let screen2 = rect(300, 0, 300, 30);
        _ = mgr.handle_event(SpaceExposed(space1, screen1.size));
        _ = mgr.handle_event(SpaceExposed(space2, screen2.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(
            space1,
            pid,
            vec![
                (WindowId::new(pid, 1), win_info()),
                (WindowId::new(pid, 2), win_info()),
            ],
        ));
        _ = mgr.handle_event(WindowsOnScreenUpdated(
            space2,
            pid,
            vec![
                (WindowId::new(pid, 3), win_info()),
                (WindowId::new(pid, 4), win_info()),
            ],
        ));
        _ = mgr.handle_event(WindowFocused(vec![space1, space2], WindowId::new(pid, 3)));
        _ = mgr.handle_event(WindowFocused(vec![space1, space2], WindowId::new(pid, 1)));

        // Test moving focus between screens.
        assert_eq!(
            mgr.handle_command(Some(space1), &[space1, space2], MoveFocus(Direction::Right))
                .focus_window,
            Some(WindowId::new(pid, 2))
        );
        _ = mgr.handle_event(WindowFocused(vec![space1, space2], WindowId::new(pid, 2)));
        assert_eq!(
            mgr.handle_command(Some(space1), &[space1, space2], MoveFocus(Direction::Right))
                .focus_window,
            Some(WindowId::new(pid, 3))
        );
        _ = mgr.handle_event(WindowFocused(vec![space1, space2], WindowId::new(pid, 3)));
        assert_eq!(
            mgr.handle_command(Some(space2), &[space1, space2], MoveFocus(Direction::Left))
                .focus_window,
            Some(WindowId::new(pid, 2))
        );
        _ = mgr.handle_event(WindowFocused(vec![space1, space2], WindowId::new(pid, 3)));

        // Test moving a node between screens.
        _ = mgr.handle_command(Some(space1), &[space1, space2], MoveNode(Direction::Right));
        mgr.debug_tree(space2);
        assert_eq!(
            vec![(WindowId::new(pid, 1), rect(0, 0, 300, 30)),],
            mgr.layout_sorted(space1, screen1),
        );
        assert_eq!(
            vec![
                // Note that 2 is moved to the right of 3.
                (WindowId::new(pid, 2), rect(400, 0, 100, 30)),
                (WindowId::new(pid, 3), rect(300, 0, 100, 30)),
                (WindowId::new(pid, 4), rect(500, 0, 100, 30)),
            ],
            mgr.layout_sorted(space2, screen2),
        );
        assert_eq!(Some(WindowId::new(pid, 2)), mgr.selected_window(space2));

        // Finally, test moving focus after moving the node.
        assert_eq!(
            mgr.handle_command(Some(space2), &[space1, space2], MoveFocus(Direction::Right))
                .focus_window,
            Some(WindowId::new(pid, 4))
        );
    }

    #[test]
    fn it_resizes_windows_with_resize_command() {
        use LayoutCommand::*;
        use LayoutEvent::*;
        let mut mgr = LayoutManager::new();
        let space = SpaceId::new(1);
        let pid = 1;
        let windows = make_windows(pid, 2);

        let screen = rect(0, 0, 100, 100);
        _ = mgr.handle_event(SpaceExposed(space, screen.size));
        _ = mgr.handle_event(WindowsOnScreenUpdated(space, pid, windows));
        _ = mgr.handle_event(WindowFocused(vec![space], WindowId::new(pid, 1)));

        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 50, 100)),
                (WindowId::new(pid, 2), rect(50, 0, 50, 100)),
            ],
            mgr.layout_sorted(space, screen),
        );

        _ = mgr.handle_command(
            Some(space),
            &[space],
            Resize {
                direction: Direction::Right,
                percent: 10.0,
            },
        );
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 60, 100)),
                (WindowId::new(pid, 2), rect(60, 0, 40, 100)),
            ],
            mgr.layout_sorted(space, screen),
        );

        _ = mgr.handle_command(Some(space), &[space], MoveFocus(Direction::Right));
        _ = mgr.handle_command(
            Some(space),
            &[space],
            Resize {
                direction: Direction::Left,
                percent: 10.0,
            },
        );
        assert_eq!(
            vec![
                (WindowId::new(pid, 1), rect(0, 0, 50, 100)),
                (WindowId::new(pid, 2), rect(50, 0, 50, 100)),
            ],
            mgr.layout_sorted(space, screen),
        );
    }
}
