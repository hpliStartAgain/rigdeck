//! RigDeck Desktop — Tauri shell entry point.
//!
//! IPC handlers are kept thin. All business behavior belongs in shared Rust crates.

use tracing_subscriber;

/// Initialize and run the Tauri application.
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RigDeck desktop application");
}

/// Simple health-check IPC command.
#[tauri::command]
fn ping() -> &'static str {
    "rigdeck-pong"
}
