# Changelog

## [0.2.0](https://github.com/glide-wm/glide/compare/v0.1.1...v0.2.0) (2026-01-08)


### ⚠ BREAKING CHANGES

* Disable exec_cmd unless a cargo feature is enabled
* group_indicators config was renamed to group_bars

### Features

* **client:** Add "config update --watch" ([82bbaf1](https://github.com/glide-wm/glide/commit/82bbaf196b173f9df8e51248ff6c58f304b336c2))
* **client:** Add "config update" command ([1e6176f](https://github.com/glide-wm/glide/commit/1e6176f528327dc90ddbe616929b406c7b13c42d))
* **client:** Add "config verify" ([864c3cd](https://github.com/glide-wm/glide/commit/864c3cdc5d30f3cf4e09541e044a43252be724ed))
* **client:** Reconnect to server during "config update --watch" ([eb4fadb](https://github.com/glide-wm/glide/commit/eb4fadb037903075b1d4fae64af9ad84b2b59193))
* **config:** Support updating WM config ([aa775f8](https://github.com/glide-wm/glide/commit/aa775f8814d87bf021862bd5293311f2f4538d20))
* Introduce glide cli ([5980f91](https://github.com/glide-wm/glide/commit/5980f9196157bb601b444c111d38154baef5a5e9))


### Bug Fixes

* **ci:** Actually codesign release again ([91014dd](https://github.com/glide-wm/glide/commit/91014dd7efb7b081b09a018265115791d77c650f))
* **ci:** Correct version bump behavior ([7e86e6b](https://github.com/glide-wm/glide/commit/7e86e6b1c0797ebeee5925d3b278385786540cb9))
* **ci:** Work around codesign issue ([b84e1c5](https://github.com/glide-wm/glide/commit/b84e1c591be5c3209ca3f32b82fe8bf592950cb4))
* Disable exec_cmd unless a cargo feature is enabled ([5a22894](https://github.com/glide-wm/glide/commit/5a228943f14276427994a832482ff9dc0a499117))

## [0.1.1](https://github.com/glide-wm/glide/compare/v0.1.0...v0.1.1) (2026-01-05)

### Bug Fixes

* **config:** Some config values were being ignored ([18abdbd](https://github.com/glide-wm/glide/commit/18abdbd7a4a71257cc7c27ac2f30bf3b61686ee6))
* **reactor:** Don't panic on windows unknown to accessibility ([f030120](https://github.com/glide-wm/glide/commit/f0301207d03fe68a5140d8da48d4f70df868d69d))

## 0.1.0 (2026-01-04)

This is the first official release of Glide, a tiling window manager for macOS.

Glide is inspired by Sway and i3 on Linux. Features include:

* Spaces support
* Keyboard-based navigation
* Tiled and untiled windows
* Tabbed and stacked groups with navigation bars
* Enable/disable on individual spaces
* Text based config
* Animations

For a better idea of what Glide can do, have a look at the [default config].

[default config]: https://github.com/glide-wm/glide/blob/3cc588bdd22cf65dc33c4e5a3afe4e6b840c41f9/glide.default.toml
