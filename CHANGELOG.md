# Changelog

## [0.2.6](https://github.com/glide-wm/glide/compare/v0.2.5...v0.2.6) (2026-01-16)


### Features

* add menubar menu with initial `quit` ([#88](https://github.com/glide-wm/glide/issues/88)) ([d6b1874](https://github.com/glide-wm/glide/commit/d6b1874a5a56d2a1bd66204d79f3af1143cd688b))


### Bug Fixes

* **cli:** Don't require server for `glide config verify` ([96c80cc](https://github.com/glide-wm/glide/commit/96c80cc6df8b723d65c5813a5823a715d1ac03de))
* **cli:** Report key parsing errors without panicking ([0f7c809](https://github.com/glide-wm/glide/commit/0f7c809a6dbf3b17ca1bfb9f1a8439752e457b51))
* Don't panic on show_timing command ([999dcdf](https://github.com/glide-wm/glide/commit/999dcdf38fd7aa6044b368504be0f925a5b7d0ce))


### Improvements

* **config:** Remove useless developer commands from default config ([dcb9053](https://github.com/glide-wm/glide/commit/dcb90533f3723c5949f71b601118a30739b1ca19))


### Experimental Features

* **status_icon:** Disable color by default ([266f74e](https://github.com/glide-wm/glide/commit/266f74e953ec35c35250b5ca2fecfab29cdea367))
* **status_icon:** space_index config now enables the space number ([3c01d93](https://github.com/glide-wm/glide/commit/3c01d934ffa0d939b59738d7dd34e4754e4d9e96))

## [0.2.5](https://github.com/glide-wm/glide/compare/v0.2.4...v0.2.5) (2026-01-13)


### Features

* **cli:** Add --restore option to `glide launch` ([b5cb685](https://github.com/glide-wm/glide/commit/b5cb685aea0d0a95ad074e74055a99a8eb8807c8))
* **cli:** Add service install/uninstall commands ([1663481](https://github.com/glide-wm/glide/commit/16634819dc5d50a4a5a84df36423f5c19c3f46be))
* **cli:** Add `--config` flag for custom config path support ([#79](https://github.com/glide-wm/glide/issues/79)) ([24f132b](https://github.com/glide-wm/glide/commit/24f132b22ad58cb964054e248f987c220532a7c3))

### Bug Fixes

* **layout:** Ensure fullscreen windows respect `outer_gap` configuration ([#80](https://github.com/glide-wm/glide/issues/80)) ([54af546](https://github.com/glide-wm/glide/commit/54af546128a1bd6819618bc90a07b5ec6665b856))

### Improvements

* **cli:** Add help text for config subcommands ([bb3a1a4](https://github.com/glide-wm/glide/commit/bb3a1a4edb4525821608b19ec9be60a14f4b99da))

### Developer Tools

* Add `app run` devtool command to run the app actor ([f6d4f14](https://github.com/glide-wm/glide/commit/f6d4f1453846ce6802e7693154b5edc148c5cc62))
* Make exec_cmd warning more precise ([e57a86b](https://github.com/glide-wm/glide/commit/e57a86b734ec13facc7c40addf71bbedb78e26c1))

### New Contributors

* @y3owk1n made their first contribution in https://github.com/glide-wm/glide/pull/79


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
