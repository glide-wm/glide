# Copilot instructions for Glide

## Build, test, and lint commands

Glide is a Rust workspace centered on the `glide-wm` crate. The default binary is `glide_server`, but coding agents should not run the live window manager directly. Use build, test, record/replay, and `devtool` workflows instead.

Primary commands from the repository root:

```bash
cargo fmt --check --verbose
cargo check --verbose --locked --target aarch64-apple-darwin
cargo build --verbose --locked
cargo test --verbose --locked
```

Run a single Rust test by name:

```bash
cargo test default_config_is_valid -- --exact
cargo test prepare_modify_clones_shared_layouts -- --exact
```

Useful developer commands:

```bash
cargo run --example devtool
cargo run --example devtool -- list ax
RUST_LOG=info,glide_wm::actor::layout=debug cargo run --example devtool -- replay traces/trace.ron
```

For automation and Copilot sessions, do not run `cargo run`, `cargo run --release`, or `glide launch` because they start the live window manager. If you need runtime investigation, prefer tests, record/replay artifacts supplied by the user, or `devtool`.

Packaging is macOS-specific and follows CI:

```bash
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin
cargo packager --release --target aarch64-apple-darwin
cargo packager --release --target x86_64-apple-darwin
```

For the documentation site:

```bash
cd site
npm run dev
npm run build
npm run preview
```

## High-level architecture

The core layering is strict: `actor -> model -> sys`, with `model` allowed to depend on `sys` only for geometry types. `config` is orthogonal and feeds both `actor` and `model`. Keep business logic out of `sys`, and keep side effects and OS interaction out of `model`.

The `actor` layer is an event-driven runtime. `WmController` orchestrates the app, hotkeys, config reload, and space enablement. `Reactor` is the central hub that reconciles app events, mouse input, screen and space changes, and commands into a coherent view of state, then drives side effects back out to app threads and UI actors.

The `LayoutManager` is not a separate process-level actor. It lives inside the Reactor and converts cleaned-up events into `LayoutTree` operations, floating-window state, layout selection, viewport updates, and frame calculations.

The `model` layer is built around `LayoutTree`, a generic slotmap-backed tree with observer-style components for sizing, selection, and window mappings. Tree and scroll layouts share the same tree structure. Scroll mode adds viewport state and animation, but still routes through the same layout model and `LayoutManager`.

Layouts are tracked per space and per screen size through `SpaceLayoutMapping`. Unmodified layouts are reused across sizes, while `prepare_modify()` performs copy-on-write cloning when a shared layout is about to change. This is important for preserving separate user-customized layouts across monitor configurations.

The `sys` layer wraps macOS APIs and infrastructure: accessibility, window server integration, screens and spaces, event taps, timers, and a single-threaded executor integrated with `CFRunLoop`. Many tests can still run without live macOS interaction because the stateful logic is pushed up into `model` and `actor` test harnesses.

## Key conventions

Configuration defaults come from `glide.default.toml`, not handwritten `Default` implementations. When adding config types, follow the `PartialConfig!` pattern so defaults, merge behavior, and validation stay aligned with the TOML source of truth.

Validate external inputs at config boundaries. This codebase prefers range clamps and filtering in `validated()` methods, then defense-in-depth caps in logic that iterates over externally influenced values.

The model layer is expected to stay deterministic. Do not call `Instant::now()`, perform I/O, or read system state inside `model`; pass time or external data in from the actor layer.

Actor communication uses unbounded Tokio MPSC channels wrapped by `actor::Sender`, which attaches the current `tracing::Span` to every message. Preserve that pattern when creating new actor message paths.

Send errors on actor channels are usually ignored intentionally during shutdown. Do not add noisy error handling for ordinary channel closure unless the call site truly requires different behavior.

Reactor consistency depends on `TransactionId`. Requests that write window frames increment a per-window transaction, and stale frame events are ignored later. Preserve this monotonic write/read contract when changing frame update flows.

Tests are mostly inline unit and integration-style tests inside the Rust modules they exercise. Model tests assert exact layout behavior directly. Reactor tests use the `Apps` harness and helpers like `simulate_events()` and `simulate_until_quiet()` rather than sleeps or real app automation.

Reactor tests automatically record and replay event traces on drop. If you change reactor event serialization or request flow, keep record/replay compatibility in mind and check the replay path.

`LayoutManager` classifies windows before managing them. Nonstandard, nonresizable, layered, or app-specific special cases often float or remain untracked. Reuse that classification path instead of introducing one-off exceptions elsewhere.

Config reload is expected to preserve user-modified layout state. Treat config as defaults and behavior settings, not as an authoritative replacement for interactively modified layout data.
