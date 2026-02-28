# Architecture

This document describes the high-level architecture of Glide. It is intended to help new contributors orient themselves and to document the design principles that guide the codebase. For build and contribution instructions, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Layers

The codebase is organized into three layers: **sys**, **model**, and **actor**. The dependency rule flows upward:

```
actor → model → sys (geometry types only)
actor → sys
```

### sys

Thin wrappers around macOS system APIs. No business logic. This includes accessibility (`AXObserver`, `AXUIElement`), window server (`CGWindowListCopyWindowInfo`, private SkyLight framework), screens and spaces, the event tap, CFRunLoop-based timers, and a single-threaded async executor that integrates futures with `CFRunLoop`.

Types defined here, like `SpaceId`, `WindowServerId`, and `CGRect`, are used throughout the codebase.

### model

Pure data structures and algorithms. The layout tree, weight-based sizing, selection tracking, and animation models all live here. The model layer has no side effects: it does not perform I/O, call system APIs, or read the clock.

Time-dependent methods accept an `Instant` parameter from the caller instead of calling `Instant::now()` internally. This makes the model fully deterministic and testable without sleeps or mocked clocks.

The model depends on `sys` only for geometry types like `CGRect` and `CGPoint`.

### actor

Event-driven actors that own resources and drive side effects. Each actor has a channel and processes events in a loop. The actor layer is the only place where time, I/O, and OS interaction happen.

### config

Configuration types, parsing, validation, and a layered merge system. Config is orthogonal to the three main layers and used by both `model` (for layout parameters) and `actor` (for behavior settings).

### ui

Native AppKit UI components: group indicator bars, the status bar icon, and the accessibility permission flow.

## Actors

Actors communicate via unbounded MPSC channels from tokio. A custom `Sender<Event>` wrapper attaches a `tracing::Span` to every message for distributed trace correlation across actor boundaries. Send errors are silently ignored – they only happen during shutdown.

The main actors are:

| Actor | Thread | Role |
|-------|--------|------|
| **WmController** | main | Top-level orchestrator. Handles space enable/disable, hotkey registration, app thread spawning, config changes. |
| **Reactor** | dedicated | Central event processor. Maintains coherence between system and model state. |
| **App** (per-process) | dedicated | Manages one application via accessibility APIs. Reads and writes window frames, observes AX notifications. |
| **Mouse** | main | CGEvent tap for mouse events: focus-follows-mouse, cursor warping and hiding, scroll wheel. |
| **RaiseManager** | inline task | Sequences window raise requests with correct ordering and timeouts. |

Several other actors handle specific concerns: `NotificationCenter` (NSWorkspace events), `WindowServer` (window destruction via SkyLight), `Dock` (Mission Control detection), `Status` (menu bar icon), `GroupBars` (tab/stack indicator overlays), and `MessageServer` (CLI IPC via CFMessagePort).

### The Reactor

The Reactor is the central hub. Its doc comment captures its role well:

> The Reactor's job is to maintain coherence between the system and model state. It takes events from the rest of the system and builds a coherent picture of what is going on.

After processing most events, the Reactor calls into the LayoutManager to compute window frames and sends the results to app threads.

### LayoutManager

The LayoutManager is embedded in the Reactor, not a separate actor. It sits between the Reactor and the `LayoutTree` model: it receives cleaned-up events and commands, converts them into tree operations, and calculates the desired position and size of each window. It also manages floating windows.

### Event flow

Events flow inward to the Reactor from all sources, and requests flow outward to app threads:

```
NSWorkspace ──→ NotificationCenter ──→ WmController ──→ Reactor
Dock ──────────────────────────────→ WmController ──→ Reactor
Mouse (CGEventTap) ────────────────────────────────→ Reactor
App threads (per-process) ─────────────────────────→ Reactor
Hotkeys ──────────────────────────→ WmController ──→ Reactor
CLI (CFMessagePort) ──→ Server ──→ WmController ──→ Reactor
```

The Reactor produces side effects by sending requests to app threads (set window frame, raise, begin/end animation), the RaiseManager, GroupBars, and Status actors.

### Transaction-based consistency

Each window has a `TransactionId` that tracks the last write. When the Reactor sends a frame update, it increments the transaction. Events from app threads carry the last transaction the app saw. Stale reads – events that arrived before the app processed our last write – are ignored. This prevents feedback loops caused by accessibility API delays.

## The layout tree

The `LayoutTree` is the central data structure. It wraps a generic `Tree<Components>` where `Components` bundles three data observers:

- **Size**: weight-based sizing info per node
- **Selection**: selected path through the tree
- **Window**: two-way mapping between leaf nodes and `WindowId`

### Generic N-ary tree

The underlying tree is backed by a single `SlotMap<NodeId, Node>`. This gives O(1) access, stable identifiers across mutations, and lets multiple components store parallel per-node data in `SecondaryMap<NodeId, _>`. The tree is not pointer-based – it uses slotmap indices with parent/child/sibling links.

The tree uses an **observer pattern**: structural mutations fire lifecycle callbacks (`added_to_forest`, `added_to_parent`, `removing_from_parent`, `removed_child`, `removed_from_forest`) that each component handles independently. This avoids coupling between the sizing, selection, and window systems.

When a child is removed and its parent becomes empty or has a single child, the observer automatically removes the empty parent or promotes the sole child. This keeps the tree minimal without manual cleanup and chain-reacts up the tree.

**Ownership model.** `OwnedNode` is an RAII type that panics on drop if not explicitly removed, preventing accidental resource leaks. `UnattachedNode`, `DetachedNode`, and `ReattachedNode` use typestate to enforce the correct sequence of tree operations at compile time.

### Weight-based sizing

Each node has a `size: f32` (weight) and each parent tracks a `total: f32` (sum of children's weights). A node's proportion of available space is `size / parent.total`. Container kinds determine how space is distributed: `Horizontal` (left to right), `Vertical` (top to bottom), `Tabbed` (overlapping, horizontal indicator bar), and `Stacked` (overlapping, vertical indicator bar).

Layout calculation is performed by a `Visitor` that walks the tree recursively, distributing available space proportionally and emitting `(WindowId, CGRect)` pairs for each leaf.

### Layout mapping

`SpaceLayoutMapping` keeps a separate layout per screen size for each space, with copy-on-write semantics. `prepare_modify()` clones the layout only when modifying one that is shared across screen sizes. Layouts are only saved for the current screen size when they are explicitly modified by the end user. Reference counting drives garbage collection of unreferenced layouts.

## Configuration

Configuration uses a layered merge system:

1. **Defaults** are embedded at compile time from `glide.default.toml`.
2. **User config** is loaded from `~/.config/glide/glide.toml` or `~/.glide.toml`.
3. The user config is merged over defaults, and the result is validated.

A `partial_config!` macro generates a "partial" version of each config struct where all fields are `Option<T>`. This enables merging: `merge(low, high)` takes `high` when present and falls back to `low`. Validation then ensures all required fields are present and values are in range.

Config is wrapped in `Arc<Config>` and flows to all actors at startup. Config changes propagate via `ConfigChanged` events.

## Testing

### Pure model tests

The model layer's determinism makes it straightforward to test. Layout tree tests construct a tree, perform operations, calculate layout with a concrete `Config`, and assert exact pixel-level frames. No mocking or timing is needed.

### Reactor integration tests

The `Apps` test harness simulates app thread responses. `simulate_events()` processes requests and generates response events. `simulate_until_quiet()` runs until no more requests are produced. This makes reactor tests self-contained.

### Record and replay

Events flowing through the Reactor can be recorded to RON files and replayed through a fresh Reactor for debugging. Every reactor test automatically replays its recording in `Drop` as consistency verification. See the CONTRIBUTING guide for usage details.

## Design principles

These principles are not always perfectly upheld, but they guide how the codebase is meant to evolve.

### Keep the model pure

The model layer should have no side effects. It should not read the clock, perform I/O, or depend on actor-layer types. When time is needed, pass `Instant` as a parameter. When identifiers are needed, prefer generic type parameters over concrete actor types.

### Validate at the boundary

Configuration values and external inputs should be validated before they reach the model layer. Clamp and filter config values in `validated()`, and add defense-in-depth caps where iteration counts depend on external input.

### Config reload should preserve user state

When configuration is hot-reloaded, only default or unmodified values should be updated. If the user has interactively resized something, reloading config should not reset it. The general principle: treat config as initial defaults, not authoritative state.

### Bound external inputs defensively

Even after validation, code that iterates based on external values should have caps. This is defense-in-depth: validation catches expected bad input, caps catch unexpected arithmetic.

When computing resize edges, drag thresholds, or hit testing, consider degenerate geometry. A window narrower than twice the resize threshold can have overlapping left and right edges.

## Scroll layout

The scroll layout is an alternative to the default tree layout, inspired by niri and PaperWM. Instead of subdividing the screen into tiles, columns extend in a horizontal strip and the user scrolls a viewport across them.

The tree supports two layout modes, selected per-space:

- **Tree** – traditional tiling with horizontal/vertical splitting (i3-style)
- **Scroll** – scrollable column layout

Both modes share the same tree structure, weight-based sizing, and selection model. The scroll mode adds a `ViewportState` per layout and routes additional events (scroll wheel, interactive resize/move) through the LayoutManager.

### Viewport and animation

`ViewportState` manages the horizontal scroll offset. It is either static or animating via a `SpringAnimation`. The spring uses a classical damped model with configurable response time and damping fraction (default: critically damped). `retarget()` preserves continuity of position and velocity when the target changes mid-animation, so rapid focus changes feel fluid rather than jerky.

Three centering modes control when the viewport scrolls to keep the focused column visible: `Always`, `OnOverflow`, and `Never`.

After layout calculation, `apply_viewport_to_frames` offsets window positions by the scroll offset and hides off-screen windows by moving them out of view. This function is generic over the window identifier type to keep the model layer free of actor-layer dependencies.

The Reactor drives animation with a timer that fires only when a scroll animation is active.

### Interactive resize and move

The LayoutManager handles interactive resize and move via mouse drag. `detect_edges` determines which edges of the focused window are near the cursor and sets the drag mode. Interactive resize works by converting pixel deltas into weight adjustments on the layout tree.
