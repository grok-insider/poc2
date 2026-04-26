//! poc2-desktop — Tauri 2 entry point.
//!
//! This is intentionally thin at M1: it boots the runtime, registers a `ping`
//! command for the frontend to verify the IPC bridge, and wires up the
//! Tauri plugins we need for clipboard / fs / http / shell.
//!
//! Application logic lives in the workspace crates (`poc2-engine`,
//! `poc2-advisor`, etc.). The Tauri layer only adapts those crates to IPC
//! commands and lifecycle events.

use tracing_subscriber::EnvFilter;

#[tauri::command]
fn ping() -> String {
    format!(
        "poc2 v{} ready (engine schema {})",
        env!("CARGO_PKG_VERSION"),
        poc2_engine::ENGINE_SCHEMA_VERSION
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,poc2=debug")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
