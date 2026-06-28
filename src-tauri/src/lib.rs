pub mod commands;
pub mod control_center;
pub mod global_shortcut;
pub mod indexer;
pub mod runtime_index;
pub mod search;

use commands::{
    get_search_index_status_command, open_file_command, read_preview_content_command,
    rebuild_search_index_command, reveal_file_command, search_file_names_command,
};
use global_shortcut::{handle_search_shortcut, ShortcutAction, ShortcutPolicy};
use runtime_index::RuntimeIndexState;
use tauri::Manager;

pub fn run() {
    let shortcut_policy = default_shortcut_policy();

    let builder = tauri::Builder::default()
        .manage(RuntimeIndexState::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([shortcut_policy.accelerator.as_str()])
                .expect("configured shortcut should register")
                .with_handler(move |app, _shortcut, _event| {
                    if let Some(ShortcutAction::ShowAndFocus { window_label }) =
                        handle_search_shortcut(&shortcut_policy, &shortcut_policy.accelerator)
                    {
                        if let Some(window) = app.get_webview_window(&window_label) {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            search_file_names_command,
            rebuild_search_index_command,
            get_search_index_status_command,
            open_file_command,
            reveal_file_command,
            read_preview_content_command
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("failed to run Maisou Tauri app");
}

fn default_shortcut_policy() -> ShortcutPolicy {
    ShortcutPolicy::from_json(include_str!("../../config/shortcut_policy.json"))
        .expect("shortcut policy configuration should parse")
}
