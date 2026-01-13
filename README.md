<!-- GUIDE_EXCLUDE_START -->
[![GHA Status]][GitHub Actions]

# Glide

Glide is a tiling window manager for macOS. It takes inspiration from window
managers like i3, Sway, and Hyprland.

<video src="https://github.com/user-attachments/assets/77570280-57ce-49f2-abc3-bf991805fad7"></video>

**Integrates with Spaces:** Adopt incrementally, using Mission Control and moving between spaces as you normally would.

**Keyboard focused; trackpad enhanced:** Bring the responsiveness of tiling window managers to macOS. Resize interactively. Speed up trackpad use with mouse-follows-focus and focus-follows-mouse.

**Adapts to your environment:** Customize your layout as you move between external monitor and on the go.

**Reliable architecture:** Made with years of experience building window managers for macOS.

Supports animations, too!


<!-- GUIDE_EXCLUDE_END -->

## Quick start

[Download the latest release][latest] from the releases page.

Open the disk image and install Glide by dragging it into Applications. I
recommend installing the glide CLI; you can do this by running in a terminal:

```
sudo ln -s /Applications/Glide.app/Contents/MacOS/glide /usr/local/bin
```

Launch the app using the CLI or with Finder.

```
glide launch
```

The first time you do this, you will have to follow instructions to enable
Accessibility permissions.

Once Glide is running, press Alt+Z to start managing the current space. Note:
This will resize all your windows! To stop managing the space, press Alt+Z again.

See [glide.default.toml] for a list of key bindings. You can customize these by
editing `~/.glide.toml` and either restarting Glide or running the following:

```
glide config update
```

> [!TIP]
> To apply changes as you save, add the `--watch` flag: `glide config update --watch`.

To exit Glide, type Alt+Shift+E.

[latest]: https://github.com/glide-wm/glide/releases/latest
[glide.default.toml]: ./glide.default.toml

### Save and restore

If you need to update Glide or restart it for any reason, exit with the
`save_and_exit` key binding (default Alt+Shift+E). Then, when starting again,
run it with the `--restore` flag:

```
glide launch --restore
```

Note that this does not work across machine restarts.

### Running Glide at login

To install Glide as a service to run at login, use:

```
glide service install
```

## Building from source

First, [install Rust](https://rustup.rs) and make sure you have the latest Xcode command line tools installed.

Then, run the following:

```
git clone https://github.com/glide-wm/glide
cd glide
cargo run --release
```

The first time you do this, you may have to follow instructions to enable
Accessibility permissions. Instead of enabling them for Glide, enable them for
whatever application your terminal is running in.

<!-- GUIDE_EXCLUDE_START -->
## Acknowledgements

Glide builds on the work of many who came before. [Yabai] contains a wealth of information
about the tricks and techniques needed to write window managers on macOS. [objc2] and related
crates make it possible for Glide to be written in Rust. [tracing] provides tree-structured
logging to follow the complex asynchronous flows that are inherently required in a macOS
window manager like Glide.

[Yabai]: https://github.com/asmvik/yabai
[objc2]: https://github.com/madsmtm/objc2
[tracing]: https://github.com/tokio-rs/tracing

## Contributing

New contributions are welcome, whether they are filing an issue, improving docs, or writing code.
See [CONTRIBUTING.md](./CONTRIBUTING.md) to get started.

#### License and usage notes

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[GitHub Actions]: https://github.com/glide-wm/glide/actions
[GHA Status]: https://github.com/glide-wm/glide/actions/workflows/test.yml/badge.svg
<!-- GUIDE_EXCLUDE_END -->
