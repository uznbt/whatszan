//! HTML settings window for Windows.
//!
//! Electron loaded `static/settings.html` in a small BrowserWindow. The Rust
//! Windows build does the same with a Tauri webview, but the page talks to the
//! Rust commands instead of Electron IPC. Linux keeps its native GTK settings
//! window in `settings_ui.rs`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use whatszan_core::translations::t;

use crate::app_state::AppState;

const LABEL: &str = "settings";

/// Open the settings window, or present the existing one.
pub fn open(app: &AppHandle) {
    let app = app.clone();
    let scheduled = app.clone();
    if let Err(err) = app.run_on_main_thread(move || present_or_create(&scheduled)) {
        tracing::error!(%err, "cannot schedule settings window");
    }
}

fn present_or_create(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let state = app.state::<AppState>();
    let app_name = state.custom_app_name();
    let title = format!(
        "{} {app_name}",
        t(&state.translations(), "settings_header_title", "Pengaturan")
    );
    let theme = if state.is_theme_dark() {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    let dark_script = if state.is_theme_dark() {
        "document.documentElement.classList.add('dark');"
    } else {
        ""
    };

    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title(title)
        .inner_size(980.0, 720.0)
        .min_inner_size(760.0, 560.0)
        .resizable(true)
        .center()
        .theme(Some(theme))
        .background_color(if state.is_theme_dark() {
            tauri::webview::Color(22, 23, 23, 255)
        } else {
            tauri::webview::Color(248, 245, 243, 255)
        })
        .initialization_script(dark_script);

    let window = match builder.build() {
        Ok(window) => window,
        Err(err) => {
            tracing::warn!(%err, "cannot create settings window");
            return;
        }
    };

    crate::webview::set_window_icon(&window);

    // Match Electron's transient settings window: it closes when the user
    // clicks back into WhatsApp. Ignore the first blur before the window has
    // ever been focused, and the blur caused by a native folder picker.
    let focused_once = Arc::new(AtomicBool::new(false));
    let event_window = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(true) => {
            focused_once.store(true, Ordering::Relaxed);
        }
        WindowEvent::Focused(false) => {
            if !focused_once.load(Ordering::Relaxed) {
                return;
            }
            let state = event_window.app_handle().state::<AppState>();
            if !state.settings_ignore_blur() {
                let _ = event_window.close();
            }
        }
        _ => {}
    });
}
