//! WhatsZan — Rust/Tauri rewrite of the original Electron client.
//!
//! The original JavaScript sources are intentionally left untouched. This
//! binary embeds the small DOM shim it needs, uses a native GTK settings window
//! on Linux and an HTML settings window on Windows, and keeps the existing
//! `WhatsZan` profile while running entirely on Rust, Tauri, WebKitGTK, or
//! WebView2.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod assets;
mod commands;
#[cfg(target_os = "linux")]
mod dbus;
mod dialogs;
mod menu;
#[cfg(target_os = "linux")]
#[path = "notifications.rs"]
mod notifications;
#[cfg(target_os = "windows")]
#[path = "notifications_windows.rs"]
mod notifications;
mod notifications_common;
mod renderer;
#[cfg(target_os = "linux")]
mod settings_ui;
#[cfg(target_os = "windows")]
mod settings_window;
#[cfg(target_os = "windows")]
mod shell_windows;
#[cfg(target_os = "linux")]
#[path = "theme.rs"]
mod theme;
#[cfg(target_os = "windows")]
#[path = "theme_windows.rs"]
mod theme;
#[cfg(target_os = "linux")]
#[path = "tray.rs"]
mod tray;
#[cfg(target_os = "windows")]
#[path = "tray_windows.rs"]
mod tray;
mod webview;
#[cfg(target_os = "windows")]
mod webview2;

use serde_json::Map;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use whatszan_core::{
    paths,
    session_backup::{backup_session_data, restore_session_backup_if_requested},
    JsonConfig,
};

use crate::app_state::AppState;

fn main() {
    // The AppUserModelID must be set before the first window is created so
    // Windows groups taskbar buttons, jump lists, and toasts under one app.
    #[cfg(target_os = "windows")]
    shell_windows::set_process_app_id();

    let args: Vec<String> = std::env::args().collect();
    let is_debug = std::env::var("DEBUG").as_deref() == Ok("1");
    let hidden_startup = args.iter().any(|arg| arg == "--hide");

    let user_data = paths::user_data_dir();

    // Session snapshots must happen before WebKit opens the profile database.
    match restore_session_backup_if_requested(&user_data) {
        Ok(true) => tracing::info!("WhatsApp session restored from snapshot"),
        Ok(false) => {
            if let Err(err) = backup_session_data(&user_data) {
                tracing::warn!(%err, "cannot create session snapshot");
            }
        }
        Err(err) => tracing::warn!(%err, "cannot restore session snapshot"),
    }

    let config = JsonConfig::new(paths::config_file(), Map::new());
    let persist = JsonConfig::new(paths::persistent_state_file(), Map::new());

    let log_level = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--log-level="))
        .map(str::to_string)
        .unwrap_or_else(|| config.get_str("log-level", "info").to_string());
    init_tracing(&log_level);

    // Logged after the subscriber exists so a broken config still leaves a
    // useful diagnostic in the journal.
    if let Some(error) = config.parse_error() {
        tracing::error!(%error, file = %config.file().display(), "cannot parse configuration");
    }
    if let Some(error) = persist.parse_error() {
        tracing::error!(%error, file = %persist.file().display(), "cannot parse persistent state");
    }

    tracing::info!(config = %config.file().display(), "starting WhatsZan");
    tracing::info!(state = %persist.file().display(), "persistent state");

    // WebKitGTK reads these before the first web process starts. WebView2 gets
    // the equivalent `--disable-gpu` switch when the main window is built.
    if config.get_bool("disable-gpu", false) {
        #[cfg(target_os = "linux")]
        {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        tracing::info!("GPU acceleration disabled by configuration");
    }

    // Electron exposes `showAtStartup` to the renderer, but always passes
    // `show: false` when the `--hide` argument is present.
    let settings_startup = args.iter().any(|arg| arg == "--settings");
    let show_at_startup = is_debug || !hidden_startup || config.get_bool("quit-on-close", false);
    // `--settings` is a settings-only launch. Keeping the main window hidden
    // prevents it from stealing focus and closing the transient settings
    // window while WhatsApp Web finishes loading.
    let window_visible = !hidden_startup && !settings_startup;
    #[cfg(target_os = "linux")]
    let enable_dbus = config.get_bool("dbus", true);

    let setup_args = args.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_second_instance(app, &argv);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::state_get,
            commands::settings_get,
            commands::settings_get_translations,
            commands::window_toggle,
            commands::escape_pressed,
            commands::open_settings,
            commands::toggle_menu,
            commands::toggle_privacy_blur,
            commands::toggle_anti_screencast,
            commands::open_url,
            commands::notify_ev,
            commands::notify_send,
            commands::log_to_main,
            commands::page_ready,
            commands::insert_text,
            commands::update_recent_chats,
            commands::update_unread,
            commands::get_pending_share,
            commands::read_file_buffer,
            commands::browse_folder,
            commands::get_default_download_paths,
            commands::check_path_exists,
            commands::settings_ignore_blur,
            commands::open_webstore,
            commands::install_extension,
            commands::screen_share_select,
            commands::settings_save,
            commands::branding_upload_icon,
            commands::branding_upload_bytes,
            commands::branding_reset_icons,
        ])
        .on_menu_event(menu::on_menu_event)
        .setup(move |app| {
            let handle = app.handle().clone();
            app.manage(AppState::new(
                user_data,
                config,
                persist,
                show_at_startup,
                is_debug,
            ));

            let dark = tauri::async_runtime::block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(2), theme::detect_theme()).await
            })
            .ok()
            .flatten()
            .unwrap_or(false);
            app.state::<AppState>().set_theme_dark(dark);

            // Register the AppUserModelId and its branding before the first
            // toast or shortcut is created.
            #[cfg(target_os = "windows")]
            shell_windows::register_app_identity(&handle);

            webview::create_main_window(&handle, window_visible)?;
            show_config_error_dialog(&handle);
            tray::spawn(handle.clone());
            #[cfg(target_os = "linux")]
            if enable_dbus {
                tauri::async_runtime::spawn(dbus::start(handle.clone()));
            }
            #[cfg(target_os = "linux")]
            tauri::async_runtime::spawn(notifications::start_reply_listener(handle.clone()));
            theme::start(handle.clone());

            if !app_state::is_wayland_session() {
                register_global_shortcuts(&handle);
            }

            commands::reapply_desktop_integration(&handle);
            handle_initial_args(&handle, &setup_args);
            spawn_signal_handler(handle.clone());

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building WhatsZan");

    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            tracing::debug!(app = %app.package_info().name, "exit requested");
        }
    });
}

/// Electron shows a Continue/Quit message box when `config.json` cannot be
/// parsed. The window already exists at this point, so the dialog is parented
/// by the desktop portal and can safely run asynchronously.
fn show_config_error_dialog(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(error) = state.config_error.clone() else {
        return;
    };
    let file = state.config_file.display().to_string();
    let translations = state.translations();
    let title = whatszan_core::translations::t(&translations, "config_title", "Configuration file")
        .to_string();
    let continue_label = whatszan_core::translations::t(
        &translations,
        "config_continue",
        "Continue with default config",
    )
    .to_string();
    let quit_label =
        whatszan_core::translations::t(&translations, "config_quit", "Quit").to_string();

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let keep_running = dialogs::message(
            dialogs::Level::Error,
            title,
            format!("{file}:\n\n{error}"),
            dialogs::Buttons::yes_no(continue_label, quit_label),
        )
        .await;
        if !keep_running {
            tracing::error!("quitting after configuration parse error");
            app.exit(0);
        }
    });
}

fn init_tracing(level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.trim().to_ascii_lowercase()));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

fn register_global_shortcuts(app: &AppHandle) {
    let blur = app
        .global_shortcut()
        .on_shortcut("CmdOrCtrl+Shift+B", |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                commands::toggle_privacy_blur(app.clone());
            }
        });
    if let Err(err) = blur {
        tracing::warn!(%err, "cannot register privacy blur shortcut");
    }

    let anti = app
        .global_shortcut()
        .on_shortcut("CmdOrCtrl+Shift+H", |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                commands::toggle_anti_screencast(app.clone());
            }
        });
    if let Err(err) = anti {
        tracing::warn!(%err, "cannot register anti screencast shortcut");
    }
}

fn handle_initial_args(app: &AppHandle, args: &[String]) {
    if let Some(url) = args.iter().find(|arg| arg.starts_with("whatsapp:")) {
        webview::handle_whatsapp_protocol(app, url);
        return;
    }
    webview::handle_jump_list_args(app, args);
    webview::handle_share_args(app, args);
    if args.iter().any(|arg| arg == "--settings") {
        webview::open_settings(app);
    }
}

fn handle_second_instance(app: &AppHandle, args: &[String]) {
    if let Some(url) = args.iter().find(|arg| arg.starts_with("whatsapp:")) {
        webview::handle_whatsapp_protocol(app, url);
        return;
    }

    if webview::handle_jump_list_args(app, args) {
        return;
    }
    if webview::handle_share_args(app, args) {
        return;
    }

    if args.iter().any(|arg| arg == "--settings") {
        webview::open_settings(app);
        return;
    }

    if args.iter().any(|arg| arg == "--hide") {
        webview::hide_main_window(app);
    } else if args.iter().any(|arg| arg == "--toggle") {
        let _ = webview::toggle_main_window(app);
    } else if args.iter().any(|arg| arg == "--quit") {
        if let Some(state) = app.try_state::<AppState>() {
            state.set_quitting(true);
        }
        app.exit(0);
    } else {
        webview::show_main_window(app);
    }
}

#[cfg(unix)]
fn spawn_signal_handler(app: AppHandle) {
    use tokio::signal::unix::{signal, SignalKind};

    for kind in [
        SignalKind::terminate(),
        SignalKind::interrupt(),
        SignalKind::hangup(),
    ] {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(mut stream) = signal(kind) else {
                return;
            };
            if stream.recv().await.is_some() {
                tracing::info!(?kind, "signal received, closing WhatsZan");
                if let Some(state) = app.try_state::<AppState>() {
                    state.set_quitting(true);
                }
                app.exit(0);
            }
        });
    }
}

#[cfg(not(unix))]
fn spawn_signal_handler(_app: AppHandle) {}
