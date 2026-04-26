// Prevents additional console window on Windows in release. Does nothing on Linux/macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    poc2_desktop::run()
}
