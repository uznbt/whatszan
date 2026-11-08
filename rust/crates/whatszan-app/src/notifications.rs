//! Native desktop notifications.
//!
//! The Electron build used `new Notification()` from Electron. On Linux the
//! same thing is an XDG desktop notification; `notify-rust` speaks that
//! protocol directly. Clicking the notification raises the main window and
//! forwards the message id to the renderer so it can open the chat.
//!
//! The icon is always the one currently used by the desktop launcher. WhatsApp
//! itself often omits `opts.icon`, and Electron's notification icon followed
//! the contact avatar instead of the app. Resolving `icon-choice-app` here
//! makes notifications keep the same logo as the launcher and pick up branding
//! changes immediately, without restarting the app.
//!
//! Inline replies are a macOS-only concept in `notify-rust`; XDG
//! notifications have no portable "reply text" action, so `notify-reply` is
//! intentionally not emitted here.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use notify_rust::{Hint, Notification, NotificationResponse};
use tauri::{AppHandle, Emitter, Manager};

use crate::{app_state::AppState, assets};

pub fn send(
    app: AppHandle,
    id: String,
    title: String,
    body: String,
    _icon_data_url: Option<String>,
) {
    tauri::async_runtime::spawn(async move {
        let (app_name, icon_path, desktop_entry) = {
            let state = app.state::<AppState>();
            (
                state.custom_app_name(),
                branding_icon(&state),
                desktop_entry_id(),
            )
        };

        let mut notification = Notification::new();
        notification
            .appname(&app_name)
            .summary(&title)
            .body(&body)
            .action("default", "Open");

        if let Some(path) = icon_path.as_deref().and_then(Path::to_str) {
            // `app_icon` is the portable notification icon; Quickshell uses
            // it for the small header logo. `image-path` covers servers that
            // only render the image hint. Both point at the same artwork.
            notification.icon(path);
            notification.hint(Hint::ImagePath(path.to_string()));
        }
        if let Some(entry) = desktop_entry {
            notification.hint(Hint::DesktopEntry(entry));
        }

        match notification.show_async().await {
            Ok(handle) => {
                handle
                    .wait_for_action_async(move |response| {
                        if matches!(
                            response,
                            NotificationResponse::Default | NotificationResponse::Action(_)
                        ) {
                            crate::webview::show_main_window(&app);
                            if let Some(window) = app.state::<AppState>().main_window() {
                                let _ = window.emit("notify-click", id.clone());
                            }
                        }
                    })
                    .await;
            }
            Err(err) => {
                tracing::warn!(%err, %title, "cannot show desktop notification");
            }
        }
    });
}

/// Resolve the icon currently used by the desktop launcher.
///
/// Branding stores the choice in `icon-choice-app` and the uploaded image in
/// `custom_tray_app`; `apply_desktop_shortcut` writes the same values into the
/// `.desktop` `Icon=` field. Reading them when a notification is sent means a
/// branding change applies to the next notification without a restart.
fn branding_icon(state: &AppState) -> Option<PathBuf> {
    let (choice, custom) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_str("icon-choice-app", "whatszan").to_string(),
            persist.get_str("custom_tray_app", "").to_string(),
        )
    };

    let bytes = if choice == "custom" && !custom.is_empty() {
        match fs::read(&custom) {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!(path = %custom, %err, "custom notification icon is unreadable");
                assets::APP_ICON_PNG.to_vec()
            }
        }
    } else if choice == "whatsapp" {
        assets::WHATSAPP_ICON_PNG.to_vec()
    } else {
        assets::APP_ICON_PNG.to_vec()
    };

    cache_icon(state, &bytes)
}

/// Store the icon under a content-addressed name so changing the branding
/// produces a new path instead of leaving notification servers with a cached
/// copy of the old logo.
fn cache_icon(state: &AppState, bytes: &[u8]) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let path = state
        .user_data
        .join("cache")
        .join(format!("notification-{:016x}.png", hasher.finish()));

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok()?;
    }
    if !path.is_file() {
        fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

/// Name of the launcher entry the notification belongs to. Quickshell and
/// other servers use this hint for grouping and for their fallback icon.
fn desktop_entry_id() -> Option<String> {
    const USER_ENTRY: &str = "WhatsZan.desktop";
    const SYSTEM_ENTRY: &str = "whatszan-native.desktop";

    let applications = dirs::data_dir()?.join("applications");
    if applications.join(USER_ENTRY).is_file() {
        return Some("WhatsZan".into());
    }
    if applications.join(SYSTEM_ENTRY).is_file()
        || Path::new("/usr/share/applications")
            .join(SYSTEM_ENTRY)
            .is_file()
    {
        return Some("whatszan-native".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_icon_follows_branding_choice() {
        let dir = std::env::temp_dir().join(format!("whatszan-notif-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let config = whatszan_core::JsonConfig::new(dir.join("config.json"), Default::default());
        let persist =
            whatszan_core::JsonConfig::new(dir.join("persistent-state.json"), Default::default());
        let state = AppState::new(dir.clone(), config, persist, false, false);

        let app_icon = branding_icon(&state).unwrap();
        assert_eq!(fs::read(&app_icon).unwrap(), assets::APP_ICON_PNG);

        let custom_path = dir.join("custom.png");
        fs::write(&custom_path, b"custom-bytes").unwrap();
        {
            let mut persist = state.persist.write().unwrap();
            persist
                .set("icon-choice-app", serde_json::json!("custom"))
                .unwrap();
            persist
                .set(
                    "custom_tray_app",
                    serde_json::json!(custom_path.display().to_string()),
                )
                .unwrap();
        }
        let custom_icon = branding_icon(&state).unwrap();
        assert_ne!(custom_icon, app_icon);
        assert_eq!(fs::read(&custom_icon).unwrap(), b"custom-bytes");

        {
            let mut persist = state.persist.write().unwrap();
            persist
                .set("icon-choice-app", serde_json::json!("whatsapp"))
                .unwrap();
        }
        let whatsapp_icon = branding_icon(&state).unwrap();
        assert_eq!(fs::read(&whatsapp_icon).unwrap(), assets::WHATSAPP_ICON_PNG);

        let _ = fs::remove_dir_all(&dir);
    }
}
