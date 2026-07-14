// Hide the console window in release; a window manager has no business owning
// a terminal, but keeping it in debug builds makes the logs visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wintilex_lib::run();
}
