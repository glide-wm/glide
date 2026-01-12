# Changelog

## [0.2.4](https://github.com/glide-wm/glide/compare/v0.2.3...v0.2.4) (2026-01-12)


### Bug Fixes

* **cli:** Ensure `glide launch` works behind a symlink ([7cd9d3e](https://github.com/glide-wm/glide/commit/7cd9d3e1b325c6335c129649e7c72b6e0287ff7b))


### Improvements

* **cli:** Add --version flag ([f56a946](https://github.com/glide-wm/glide/commit/f56a946ba8a88eb79655ff0aca7525faa129a456))

## 0.2.3 (2026-01-12)

### What's Changed
* feat(cli): Add `glide launch` command by @tmandry in https://github.com/glide-wm/glide/pull/69


**Full Changelog**: https://github.com/glide-wm/glide/compare/v0.2.2...v0.2.3

## 0.2.2 (2026-01-10)

### What's Changed
* layout: Add configurable inner/outer gaps around windows by @intergrav in https://github.com/glide-wm/glide/pull/63

### New Contributors
* @intergrav made their first contribution in https://github.com/glide-wm/glide/pull/63

**Full Changelog**: https://github.com/glide-wm/glide/compare/v0.2.1...v0.2.2

## [0.2.1](https://github.com/glide-wm/glide/compare/v0.2.0...v0.2.1) (2026-01-09)


### Bug Fixes

* **app:** Display error instead of panicking on invalid config ([1ef2a23](https://github.com/glide-wm/glide/commit/1ef2a23fe1964c10b185163a6f83f572d1cd31f5))
* **cli:** Don't panic when config file is missing ([9fbbf21](https://github.com/glide-wm/glide/commit/9fbbf2170281caf1f355f19512ddeaf6932ad82d))

## [0.2.0](https://github.com/glide-wm/glide/compare/v0.1.1...v0.2.0) (2026-01-08)


### ⚠ BREAKING CHANGES

* Disable exec_cmd unless a cargo feature is enabled
* group_indicators config was renamed to group_bars

### Features

* Introduce glide cli ([5980f91](https://github.com/glide-wm/glide/commit/5980f9196157bb601b444c111d38154baef5a5e9))
* **client:** Add "config update" command ([1e6176f](https://github.com/glide-wm/glide/commit/1e6176f528327dc90ddbe616929b406c7b13c42d))
* **client:** Add "config update --watch" ([82bbaf1](https://github.com/glide-wm/glide/commit/82bbaf196b173f9df8e51248ff6c58f304b336c2))
* **client:** Add "config verify" ([864c3cd](https://github.com/glide-wm/glide/commit/864c3cdc5d30f3cf4e09541e044a43252be724ed))


### Bug Fixes

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
