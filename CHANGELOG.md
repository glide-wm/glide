# Changelog

## [0.1.1](https://github.com/glide-wm/glide/compare/v0.1.0...v0.1.1) (2026-01-05)


### Features

* **client:** Add "config update --watch" ([c564901](https://github.com/glide-wm/glide/commit/c56490174035937f22757fb6616ff268dd5f9b35))
* **client:** Add "config update" command ([d47f52a](https://github.com/glide-wm/glide/commit/d47f52a6e2e65a5796db17a618891329532755ea))
* **client:** Add "config verify" ([467abcb](https://github.com/glide-wm/glide/commit/467abcb883581401bae594ab6aa95f2fb2a0fc34))
* **client:** Reconnect to server during "config update --watch" ([b879fb5](https://github.com/glide-wm/glide/commit/b879fb5b41ad3a2ce1367c4177739f42b344b7f7))
* **config:** Support updating WM config ([d5bb36b](https://github.com/glide-wm/glide/commit/d5bb36ba2266777107ee5e80608dba2d4601b415))
* Introduce glide cli ([8f76cde](https://github.com/glide-wm/glide/commit/8f76cde272faf5258010a990edded98ff2fbb8aa))


### Bug Fixes

* **config:** Some config values were being ignored ([18abdbd](https://github.com/glide-wm/glide/commit/18abdbd7a4a71257cc7c27ac2f30bf3b61686ee6))
* **config:** Some config values were being ignored ([bb15051](https://github.com/glide-wm/glide/commit/bb15051a87459e81ae50e49dd57dc39fe402b149))
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
