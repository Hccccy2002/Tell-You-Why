mod auto_hide;
mod commands;
mod content;
mod db;
mod desktop;
mod evaluation;
mod evaluation_fixture;
mod evaluation_live;
mod evaluation_review;
mod harness;
mod knowledge_base;
mod learning;
mod learning_commands;
mod learning_generation;
#[cfg(test)]
mod learning_live_tests;
mod learning_store;
mod models;
mod navigation;
mod pdf_runtime;
mod persistence_gate;
mod providers;
mod rag;
mod rag_commands;
#[cfg(test)]
mod rag_live_tests;
mod rag_store;
mod review_agent;
mod review_commands;
mod review_eval;
mod review_memory;
#[cfg(test)]
mod review_memory_tests;
mod review_store;
#[cfg(test)]
mod review_tests;
mod review_tools;
mod review_trace;
mod secret_store;
mod study;
mod study_agent;
mod study_commands;
mod study_doubts;
mod study_goal;
#[cfg(test)]
mod study_goal_tests;
mod study_store;
#[cfg(test)]
mod study_tests;
#[cfg(test)]
mod textbook_eval;

use db::Database;
use persistence_gate::PersistenceResetGate;
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
    pub auto_hide: auto_hide::AutoHideController,
    pub(crate) persistence_gate: PersistenceResetGate,
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
            database
                .rag_recover_interrupted()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            database
                .review_recover()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            database
                .study_recover()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let http = RestrictedHttpClient::new()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(AppState {
                database,
                secrets: Arc::new(WindowsCredentialStore),
                http: Arc::new(http),
                exiting: AtomicBool::new(false),
                generation_in_progress: AtomicBool::new(false),
                auto_hide: auto_hide::AutoHideController::default(),
                persistence_gate: PersistenceResetGate::default(),
            });
            app.manage(navigation::NavigationState::default());
            desktop::setup(app)?;
            app.manage(knowledge_base::KnowledgeBaseState::default());
            app.manage(evaluation::EvaluationState(
                evaluation::EvaluationManager::open(data_dir.join("evaluations")).map(Arc::new),
            ));
            if std::env::args_os().any(|argument| argument == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    window.state::<AppState>().auto_hide.window_hidden();
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if let Some(webview_window) = window.app_handle().get_webview_window("main") {
                        desktop::handle_close_requested(&webview_window, api);
                    }
                }
                tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                    window
                        .app_handle()
                        .state::<AppState>()
                        .auto_hide
                        .protect_for_interaction();
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            navigation::take_window_navigation,
            navigation::acknowledge_window_navigation,
            study_commands::study_start,
            study_commands::study_start_goal,
            study_commands::study_goal_checkin,
            study_commands::study_reopen_goal,
            study_commands::study_continue,
            study_commands::study_latest,
            study_commands::study_home,
            study_commands::study_start_review,
            study_commands::study_start_card,
            study_commands::study_start_card_view,
            study_commands::study_card_sessions,
            study_commands::study_save_highlight,
            study_commands::study_highlights,
            study_commands::study_remove_highlight,
            study_commands::study_question_feedback,
            study_commands::study_start_doubt,
            study_commands::study_source_card,
            study_commands::study_ask,
            study_commands::study_read,
            study_commands::study_feedback,
            study_commands::study_pause,
            study_commands::study_history,
            study_commands::study_reset,
            evaluation::evaluation_list,
            evaluation::evaluation_start,
            evaluation::evaluation_read,
            evaluation::evaluation_cancel,
            evaluation::evaluation_export,
            evaluation::evaluation_save_review,
            evaluation::evaluation_benchmark,
            evaluation::evaluation_benchmark_export,
            knowledge_base::kb_read,
            knowledge_base::kb_import,
            knowledge_base::kb_resume,
            knowledge_base::kb_pause,
            rag_commands::rag_providers,
            rag_commands::rag_prepare,
            learning_generation::rag_prepare_random,
            rag_commands::rag_generate,
            rag_commands::rag_related_sources,
            rag_commands::rag_list,
            review_commands::review_start,
            review_commands::review_continue,
            review_commands::review_latest,
            review_commands::review_read,
            review_commands::review_answer,
            review_commands::review_cancel,
            review_commands::review_trace,
            review_commands::review_history,
            review_commands::review_memory,
            review_commands::review_export,
            learning_commands::learning_start,
            learning_commands::learning_resume,
            learning_commands::learning_next,
            learning_commands::learning_previous,
            learning_commands::learning_record_event,
            learning_commands::learning_reset,
            commands::bootstrap_app,
            commands::next_card,
            commands::available_card_count,
            commands::record_interaction,
            commands::save_onboarding,
            commands::save_interests,
            commands::list_library,
            commands::delete_library_card,
            commands::save_settings,
            commands::set_auto_hide_suspended,
            commands::model_operation_busy,
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
            commands::ask_follow_up,
            commands::list_card_follow_ups,
            commands::import_cards_file,
            commands::clear_data,
            commands::open_source_url,
            commands::exit_application,
        ])
        .run(tauri::generate_context!())
        .expect("Tell You Why 启动失败");
}

/// Check the installed PDF adapter and write a diagnostic JSON report.
pub fn check_pdf(output: &std::path::Path) -> Result<(), String> {
    let result = (|| {
        let paths = pdf_runtime::Layout::discover()?;
        let catalog =
            knowledge_base::Runtime::discover()?.call(serde_json::json!({"op":"catalog"}))?;
        if catalog["models_ready"] != true {
            return Err("本地模型不完整".to_string());
        }
        Ok(
            serde_json::json!({"ok":true,"python":paths.python,"service":paths.service,"data":paths.data,"models":paths.models,"catalog":catalog}),
        )
    })();
    let value = match &result {
        Ok(v) => v.clone(),
        Err(e) => serde_json::json!({"ok":false,"error":e}),
    };
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    result.map(|_| ())
}
