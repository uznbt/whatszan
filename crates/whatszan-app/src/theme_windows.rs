//! Windows system theme monitoring.
//!
//! Windows stores the app/light preference in the current user's registry:
//! `AppsUseLightTheme = 0` means dark mode. Polling the value every five
//! seconds mirrors the Linux portal watcher and keeps the title bar, window
//! background, and settings page in sync without requiring a restart.

use std::time::Duration;

use tauri::{AppHandle, Manager};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

use crate::app_state::AppState;

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<bool> = None;
        loop {
            let dark = detect_theme().await;
            if let Some(dark) = dark {
                if last != Some(dark) {
                    apply(&app, dark);
                    last = Some(dark);
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

/// Read the preferred color scheme. `Some(true)` means dark.
pub async fn detect_theme() -> Option<bool> {
    tokio::task::spawn_blocking(read_theme).await.ok().flatten()
}

fn read_theme() -> Option<bool> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .ok()?;
    let apps_use_light: u32 = key.get_value("AppsUseLightTheme").ok()?;
    Some(apps_use_light == 0)
}

fn apply(app: &AppHandle, dark: bool) {
    let state = app.state::<AppState>();
    if !state.set_theme_dark(dark) {
        return;
    }

    let theme = if dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    for label in ["main", "settings"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.set_theme(Some(theme));
        }
    }
    // The HTML settings page switches its palette with a `dark` class rather
    // than `prefers-color-scheme`, so update it explicitly while it is open.
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.eval(format!(
            "document.documentElement.classList.toggle('dark', {dark});"
        ));
    }
    tracing::debug!(dark, "system theme changed");
}
