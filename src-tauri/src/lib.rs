mod accounts;
mod ai;
mod avatars;
mod calendar;
mod commands;
mod compose;
mod contacts;
mod db;
mod desktop;
mod error;
#[cfg(target_os = "linux")]
mod graphics;
mod imap;
mod links;
#[cfg(target_os = "linux")]
mod session;
mod mail;
mod mail_auth;
mod oauth;
mod proton;
mod secrets;
mod smtp;
mod spaces;
mod tabs;
mod theme;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::oauth::flow::OAuthFlowState;
pub use error::AppError;
use secrets::{KeyringStore, MemoryStore, SecretStore};

pub struct AppState {
    pub db: Arc<db::Database>,
    pub secrets: Arc<dyn SecretStore>,
    pub syncing: Arc<Mutex<HashSet<i64>>>,
    pub oauth_flow: Arc<OAuthFlowState>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        session::ensure_session_environment();
        graphics::apply_graphics_workarounds();
    }

    tauri::Builder::default()
        // Must be registered before other plugins. A launch from a second
        // process focuses the existing instance instead of creating another.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let database = Arc::new(db::Database::open(&data_dir.join("auramail.db"))?);
            let secrets: Arc<dyn SecretStore> =
                if std::env::var_os("AURAMAIL_MEMORY_SECRETS").is_some() {
                    Arc::new(MemoryStore::default())
                } else {
                    Arc::new(KeyringStore)
                };
            let syncing = Arc::new(Mutex::new(HashSet::new()));
            let oauth_flow = Arc::new(OAuthFlowState::new());

            app.manage(AppState {
                db: Arc::clone(&database),
                secrets: Arc::clone(&secrets),
                syncing: Arc::clone(&syncing),
                oauth_flow: Arc::clone(&oauth_flow),
            });

            let proton_dir = data_dir.join("proton-bridge");
            let proton_sidecar = proton::sidecar_path().unwrap_or_else(|error| {
                // Keep the bridge installed so Proton commands report a clear
                // "unavailable in this build" error instead of silently
                // disappearing from the app.
                eprintln!("{error}");
                proton_dir.join("auramail-proton-bridge-unavailable")
            });
            proton::runtime::install(Arc::new(proton::ProtonBridge::new(
                Arc::new(proton::TauriProtonEvents::new(app.handle().clone())),
                Arc::clone(&database),
                Arc::clone(&secrets),
                proton_dir,
                proton_sidecar,
            )));

            desktop::install_notification_actions(app.handle());
            if let Err(error) = theme::ensure_config(app.handle()) {
                eprintln!("theme config initialization failed: {error}");
            }
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                let nvidia = graphics::nvidia_module_loaded();
                let _ = window.with_webview(move |webview| {
                    graphics::configure_webkit_memory(webview.inner());
                    if nvidia {
                        graphics::disable_webkit_flicker_features(webview.inner());
                    }
                });
            }
            if let Ok(first_space) = spaces::list(&database).map(|spaces| spaces.into_iter().next()) {
                if let Some(space) = first_space {
                    if let Err(error) =
                        theme::set_active_theme_space(&database, app.handle(), space.id)
                    {
                        eprintln!("theme watcher failed to start: {error}");
                    }
                }
            }
            if let Err(error) = proton::runtime::start_if_needed(&database) {
                eprintln!("Proton bridge failed to start: {error}");
            }
            imap::sync::start_polling(app.handle().clone(), Arc::clone(&database), secrets, syncing);
            #[cfg(target_os = "linux")]
            {
                if desktop::tray_available() {
                    if let Err(error) = desktop::install_tray(app.handle()) {
                        eprintln!("system tray unavailable: {error}");
                    }
                } else {
                    eprintln!(
                        "system tray unavailable: install libayatana-appindicator3 or run through the Nix shell"
                    );
                }
            }
            #[cfg(not(target_os = "linux"))]
            desktop::install_tray(app.handle())?;
            desktop::refresh_tray_unread(app.handle(), &database);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if desktop::close_to_tray(&window.app_handle().state::<AppState>().db) {
                        api.prevent_close();
                        let _ = window.hide();
                    } else {
                        window.app_handle().exit(0);
                    }
                }
                tauri::WindowEvent::Resized(_) => {
                    if window.is_minimized().unwrap_or(false)
                        && desktop::minimize_to_tray(&window.app_handle().state::<AppState>().db)
                    {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::list_spaces,
            commands::space_unread_counts,
            commands::create_space,
            commands::rename_space,
            commands::delete_space,
            commands::reorder_accounts,
            commands::move_account,
            commands::google_oauth_status,
            commands::start_google_oauth,
            commands::reconnect_google_account,
            commands::cancel_google_oauth,
            commands::microsoft_oauth_status,
            commands::start_microsoft_oauth,
            commands::reconnect_microsoft_account,
            commands::cancel_microsoft_oauth,
            commands::get_ai_config,
            commands::set_ai_enabled,
            commands::ai_has_key,
            commands::discover_ai_providers,
            commands::list_ai_models,
            commands::get_ai_preferences,
            commands::set_ai_preferences,
            commands::get_ai_voice_profile,
            commands::learn_ai_voice,
            commands::clear_ai_voice_profile,
            commands::get_semantic_status,
            commands::start_semantic_index,
            commands::set_ai_config,
            commands::test_ai,
            commands::ai_status,
            commands::ai_reply,
            commands::ai_aura_rail,
            commands::ai_aura_ask,
            commands::ai_compose,
            commands::ai_chat,
            commands::list_contacts,
            commands::open_compose_window,
            commands::add_account,
            commands::start_proton_login,
            commands::continue_proton_login,
            commands::cancel_proton_login,
            commands::update_account,
            commands::set_account_color,
            commands::discover_account_settings,
            commands::delete_account,
            commands::set_sync_interval,
            commands::get_fast_sync_interval,
            commands::set_fast_sync_interval,
            commands::get_desktop_preferences,
            commands::set_desktop_preferences,
            commands::refresh_tray_unread,
            commands::sync_account,
            commands::list_calendars,
            commands::list_calendar_events,
            commands::list_calendar_accounts,
            commands::reorder_calendar_accounts,
            commands::move_calendar_account,
            commands::add_calendar_account,
            commands::remove_calendar_account,
            commands::unsubscribe_calendar,
            commands::sync_calendar,
            commands::create_calendar_event,
            commands::update_calendar_event,
            commands::delete_calendar_event,
            commands::list_folders,
            commands::list_messages,
            commands::list_threads,
            commands::list_thread_messages,
            commands::list_tabs,
            commands::tab_unread_counts,
            commands::create_tab,
            commands::update_tab,
            commands::delete_tab,
            commands::reorder_tabs,
            commands::classify_message,
            commands::classify_messages,
            commands::classify_unclassified,
            commands::get_message,
            commands::mark_read,
            commands::fill_message_snippets,
            commands::move_message_to_folder,
            commands::set_message_classification,
            commands::toggle_star,
            commands::trash_message,
            commands::archive_message,
            commands::archive_thread,
            commands::delete_message,
            commands::download_attachment,
            commands::attachment_preview,
            commands::load_remote_images,
            commands::get_sender_avatar,
            commands::get_mail_preferences,
            commands::set_mail_preferences,
            commands::send_message,
            commands::save_draft,
            commands::list_drafts,
            commands::get_draft,
            commands::delete_draft,
            commands::system_color_scheme,
            commands::get_theme_settings,
            commands::set_theme_settings,
            commands::set_active_theme_space,
            commands::reload_theme,
            commands::open_external
        ])
        .build(tauri::generate_context!())
        .expect("error while running AuraMail")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                proton::runtime::stop();
            }
        });
}
