<!-- GUIDE_EXCLUDE_START -->
[![GHA Status]][GitHub Actions]

# Glide

Glide is a tiling window manager for macOS. It takes inspiration from window
managers like i3, Sway, and Hyprland.

<!-- GUIDE_EXCLUDE_END -->

## Quick start

[Download the latest release][latest] from the releases page.

Open the disk image and install Glide by dragging it into Applications, then
launch the app. The first time you do this, you will have to follow instructions
to enable Accessibility permissions.

Once Glide is running, press Alt+Z to start managing the current space. Note:
This will resize all your windows!

See [glide.default.toml] for a list of key bindings. You can customize these by
editing `~/.glide.toml` and restarting Glide (hit Alt+Shift+E to exit, then
re-launch).

[latest]: https://github.com/glide-wm/glide/releases/latest
[glide.default.toml]: ./glide.default.toml

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

If you are interested in contributing, there are lots of tips in
[CONTRIBUTING.md](./CONTRIBUTING.md).

### Save and restore

If you need to update Glide or restart it for any reason, exit with the
`save_and_exit` key binding (default Alt+Shift+E). Then, when starting again,
run it with the `--restore` flag:

```
cargo run --release -- --restore
```

Note that this does not work across machine restarts. Currently it only works
when running Glide from the command line.

<!-- GUIDE_EXCLUDE_START -->
#### License and usage notes

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[GitHub Actions]: https://github.com/glide-wm/glide/actions
[GHA Status]: https://github.com/glide-wm/glide/actions/workflows/test.yml/badge.svg
<!-- GUIDE_EXCLUDE_END -->
