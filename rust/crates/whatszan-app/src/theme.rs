//! System theme monitoring.
//!
//! Electron's `nativeTheme` reads the Linux preference from the XDG desktop
//! portal, with `gsettings` / `kreadconfig` as a fallback. The portal is read
//! in-process through `ashpd`; subprocesses are only used when the portal is
//! unavailable. The value is applied to the windows only when it changes, so
//! the 5-second poll does not cause repaints.

use std::time::Duration;

use ashpd::desktop::settings::{ColorScheme, Settings};
use tauri::{AppHandle, Manager};
use tokio::process::Command;

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
    if let Some(dark) = read_portal().await {
        return Some(dark);
    }
    read_gsettings().await
}

async fn read_portal() -> Option<bool> {
    let settings = Settings::new().await.ok()?;
    match settings.color_scheme().await.ok()? {
        ColorScheme::PreferDark => Some(true),
        ColorScheme::PreferLight => Some(false),
        ColorScheme::NoPreference => None,
    }
}

async fn read_gsettings() -> Option<bool> {
    if let Some(output) = command_output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
    )
    .await
    {
        if output.contains("prefer-dark") {
            return Some(true);
        }
        if output.contains("prefer-light") {
            return Some(false);
        }
    }

    if let Some(output) = command_output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "gtk-theme"],
    )
    .await
    {
        let lower = output.to_ascii_lowercase();
        if lower.contains("dark") && !lower.contains("adw-gtk3") {
            return Some(true);
        }
    }

    // KDE Plasma stores the window background as "R,G,B" in kreadconfig.
    let kde = command_output(
        "kreadconfig6",
        &["--group", "Colors:Window", "--key", "BackgroundNormal"],
    )
    .await
    .or(command_output(
        "kreadconfig5",
        &["--group", "Colors:Window", "--key", "BackgroundNormal"],
    )
    .await);

    if let Some(output) = kde {
        let parts: Vec<f64> = output
            .trim()
            .split(',')
            .filter_map(|part| part.trim().parse::<f64>().ok())
            .collect();
        if parts.len() == 3 {
            return Some(parts.iter().sum::<f64>() < 384.0);
        }
    }

    None
}

async fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
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
    tracing::debug!(dark, "system theme changed");
}
