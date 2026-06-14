# Tilex

Auto-tiling window manager for Windows.

Tilex watches the desktop through the Win32 event hooks, keeps track of every
manageable top-level window and re-tiles them whenever something opens, closes
or moves. The window management runs entirely in Rust; Tauri only provides the
settings window and the tray icon.

## Status

Early. Works, but expect rough edges.

## Building

```
npm install
npm run tauri dev
```

Requires Rust (stable, MSVC toolchain), Node 18+ and the WebView2 runtime.

## License

MIT
