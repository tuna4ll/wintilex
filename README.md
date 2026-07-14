<img src="assets/logo.svg" alt="" width="72" align="left" hspace="12">

# WinTilex

Auto-tiling window manager for Windows.

WinTilex watches the desktop through the Win32 event hooks, keeps track of every
manageable top-level window and re-tiles them whenever something opens, closes
or moves. The window management runs entirely in Rust on its own message-loop
thread; Tauri only provides the settings window and the tray icon.

- One window fills the work area, two split it in half, and beyond that the
  screen is cut recursively so every window ends up with roughly the same area.
- Every display gets its own window list, layout and split ratios.
- Windows can be floated per application through a rule, or one at a time with
  a hotkey.
- Dragging a tile edge changes the split behind it instead of being undone on
  the next pass, so manual resizing sticks.

## Building

```
npm install
npm run tauri dev      # development
npm run tauri build    # installers land in target/release/bundle
```

Requires Rust (stable, MSVC toolchain), Node 18 or newer, and the WebView2
runtime, which ships with Windows 11.

The window management can also be run on its own, without any UI:

```
cargo run -p wintilex-core --example headless -- 30
cargo run -p wintilex-core --example list_windows
```

## Default hotkeys

| Binding                      | Action                                     |
| ---------------------------- | ------------------------------------------ |
| `Win` + arrow keys            | Move the focus left, down, up, right       |
| `Win` `Shift` + arrows        | Swap the focused window with its neighbour |
| `Win` `Alt` + arrows          | Grow the focused window that way           |
| `Win` `Alt` `Shift` + `←` / `→`   | Send the window to the next display        |
| `Win` `Alt` + `J` / `K`       | Focus the next / previous window           |
| `Win` `Alt` + `Enter`         | Promote the window to first place          |
| `Win` `Alt` + `Space`         | Switch to the next layout                  |
| `Win` `Alt` + `F`             | Float or unfloat the focused window        |
| `Win` `Alt` + `X`             | Mirror the layout                          |
| `Win` `Alt` + `Z`             | Forget every manual resize on this display |
| `Win` `Alt` + `P`             | Pause or resume tiling                     |
| `Win` `Alt` + `Q`             | Minimize the focused window                |

The directional actions sit on the arrows because that is the part of the
keyboard Windows already spends on window arranging: `Win`+arrow snaps a window
to half the screen and `Win`+`Shift`+arrow throws it at the next display. WinTilex
does both of those properly, so taking them over loses nothing. Letters were
the obvious first choice, but `Win`+`L` locks the screen and no focus key is worth
that. `Win`+`Ctrl`+arrow is left alone as well, since virtual desktops have
nothing to do with tiling.

To shrink a window, grow it in the opposite direction.

Windows reserves most of these combinations for the shell, and `RegisterHotKey`
refuses them outright. WinTilex therefore captures its bindings with a low-level
keyboard hook by default, which sees the key before the shell does. The General
page can switch to `RegisterHotKey` instead; the Hotkeys page then marks
whatever Windows would not hand over.

### Shortcuts WinTilex leaves alone

Seeing keys before the shell cuts both ways: a binding on the wrong combination
would take a Windows feature away with no warning. WinTilex therefore refuses to
swallow the shortcuts the shell actually needs, whatever the config says:

`Win`+`L`, `Win`+`Tab`, `Win`+`Ctrl`+`←`/`→`/`D`/`F4`, `Win`+`Space`,
`Win`+`Shift`+`Space`, `Win`+`D`, `Win`+`G`, `Win`+`Shift`+`S`,
`Win`+`PrtScn`, `Alt`+`Tab`.

A binding on one of these is kept in the file but never registered, and the
Hotkeys page says why. Turn *Leave Windows shortcuts alone* off on the General
page to take them over anyway.

A configuration written by an older build is carried onto the current bindings
the first time it is read. Only hotkeys still sitting on a previous default are
moved; anything chosen by hand is left exactly where it is.

## Layouts

| Layout           | Shape                                                       |
| ---------------- | ----------------------------------------------------------- |
| Binary split     | Recursive halving along the longer side. The default.       |
| Columns / Rows   | Equal strips.                                               |
| Main and stack   | One large window plus the rest, in the style of dwm.        |
| Monocle          | Every window fills the screen; only the focused one shows.  |

The layout is picked per display, from the tray menu or with `Win`+`Alt`+`Space`.
Adding a new one means implementing `LayoutAlgorithm` in `crates/wintilex-core/src/layout`
and adding a variant to `LayoutKind`; reporting the split edges is what lets a
manual resize be folded back in.

## Configuration

`%APPDATA%\WinTilex\config.json`, written by the settings window and re-read when
it is saved. A missing or partly filled file is fine: every field falls back to
its default.

Rules decide what happens to a window, matched from the top with the first hit
winning:

```json
{
  "rules": [
    { "process": "taskmgr.exe", "action": "float" },
    { "class": "#32770", "action": "float" },
    { "process": "code.exe", "title-contains": "Settings", "action": "ignore" }
  ]
}
```

`tile` puts the window in the layout, `float` leaves its position alone, and
`ignore` makes WinTilex pretend it does not exist. A rule can also pin an
application to a display with `"monitor": 1`.

## Notes

- Starting with Windows is a per-user entry under the `Run` registry key, so
  turning it on never asks for elevation.
- Elevated windows cannot be moved by a normal process. Run WinTilex as
  administrator if you need it to manage them.
- Only one instance can run at a time; launching WinTilex again just brings the
  settings window back.

## License

MIT
