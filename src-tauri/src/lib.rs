mod commands;
mod content;
mod db;
mod desktop;
mod models;
mod providers;
mod secret_store;

use db::Database;
use providers::{ProviderTransport, RestrictedHttpClient};
use secret_store::{SecretStore, WindowsCredentialStore};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::ShortcutState;

pub struct AppState {
    pub database: Database,
    pub secrets: Arc<dyn SecretStore>,
    pub http: Arc<dyn ProviderTransport>,
    pub exiting: AtomicBool,
    pub generation_in_progress: AtomicBool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        desktop::toggle_window(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let database = Database::new(data_dir.join("tell-you-why.db"));
            database
                .initialize()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let http = RestrictedHttpClient::new()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(AppState {
                database,
                secrets: Arc::new(WindowsCredentialStore),
                http: Arc::new(http),
                exiting: AtomicBool::new(false),
                generation_in_progress: AtomicBool::new(false),
            });
            desktop::setup(app)?;
            if std::env::args_os().any(|argument| argument == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    if let Some(webview_window) = window.app_handle().get_webview_window("main") {
                        desktop::handle_close_requested(&webview_window, api);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap_app,
            commands::next_card,
            commands::available_card_count,
            commands::record_interaction,
            commands::save_onboarding,
            commands::save_interests,
            commands::list_library,
            commands::delete_library_card,
            commands::save_settings,
            commands::save_generation_provider,
            commands::generation_usage,
            commands::pause_reminders,
            commands::save_provider_profile,
            commands::delete_provider_key,
            commands::test_provider_connection,
            commands::generate_same_topic,
            commands::generate_topic_batch,
            commands::generate_random_topic,
            commands::generate_random_topic_batch,
            commands::import_cards_file,
            commands::clear_data,
            commands::open_source_url,
            commands::exit_application,
        ])
        .run(tauri::generate_context!())
        .expect("Tell You Why 启动失败");
}
