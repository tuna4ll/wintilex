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
- An optional bar along the edge of every display shows the layout, the windows
  and the usual readings, and reserves its own space.

## Building

```
npm install
npm run tauri dev      # development
npm run tauri build    # installers land in target/release/bundle
```

Requires Rust (stable, MSVC toolchain), Node 18 or newer, and the WebView2
runtime, which ships with Windows 11.

Run WinTilex through one of those two commands, or from the installer they
build. A debug binary started on its own — `target\debug\wintilex.exe` — loads
its interface from the Vite dev server rather than from disk, so without
`npm run tauri dev` running alongside it the settings window is only a browser
error page, and it keeps the console open for the logs.

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

## Bar

WinTilex can draw a bar along the top or bottom of every display, in the style
of the ones that come with the tiling managers on Linux. It is off until you
turn it on, from the *Bar* page in the settings window or the *Top bar* item in
the tray menu; both write the choice to the config file, so it comes back the
way you left it.

| Module          | Shows                                                        |
| --------------- | ------------------------------------------------------------ |
| Layout          | The layout of this display. Click it for the next one.       |
| Windows         | One pill per window, in tiling order. Click one to focus it. |
| Focused title   | The title of the focused window, cut with an ellipsis.       |
| Tiling paused   | Only while tiling is off. Click it to resume.                |
| Display number  | Which display the bar belongs to.                            |
| CPU, Memory     | Load and memory in use, read once a second.                  |
| Battery         | Charge and whether it is plugged in. Hidden without one.     |
| Clock           | `%H` `%I` `%M` `%S` `%p` `%d` `%m` `%y` `%Y` `%a` `%b`.       |

Each module sits on the left, in the middle or on the right, and the order
inside a side is the order they were put there. A side is drawn as one rounded
group, and the three groups float clear of the screen edge rather than filling a
strip: the bar window is layered and painted through `UpdateLayeredWindow`, so
every pixel carries its own alpha and the wallpaper shows through the gaps with
the corners antialiased.

Icons come from *Segoe Fluent Icons*, which ships with Windows, so there is
nothing to install. Point `icon-font` at a Nerd Font and paste its glyphs into
the `glyphs` section to use those instead. `battery` and `battery-charging` are
the first of ten glyphs running from empty to full.

Every module has its own accent, which is where most of the character comes
from: the icons are coloured while the text stays readable. Height, margin,
corner radius, opacity, font and all thirteen colours are in the same file as
everything else, and the shipped palette is a dark one in the style of the
Linux tiling desktops the bar is modelled on.

The bar registers itself with the shell as an appbar, which takes its strip out
of the work area. Since the layouts are already computed against the work area,
nothing else had to change for windows to stop under it — and maximized windows
and `Win`+`Arrow` snapping keep off it too, which they would not if the bar
merely floated on top. Turn *Reserve the space* off to get the floating
behaviour anyway. The registration is given back when the bar is switched off or
WinTilex quits; a bar left registered would leave a dead strip along the edge of
the screen.

The bar can be looked at without the rest of the application, on a desktop it is
not laying out:

```
cargo run -p wintilex-bar --example preview -- 20
```

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
