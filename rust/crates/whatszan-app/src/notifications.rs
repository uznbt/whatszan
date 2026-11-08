//! Native desktop notifications.
//!
//! The Electron build used `new Notification()` from Electron. On Linux the
//! same thing is an XDG desktop notification; `notify-rust` speaks that
//! protocol directly. Clicking the notification raises the main window and
//! forwards the message id to the renderer so it can open the chat.
//!
//! Two images are involved:
//!
//! * `app_icon` is always the branding icon currently used by the desktop
//!   launcher (`icon-choice-app`). WhatsApp often omits this icon entirely,
//!   and Electron's notification icon followed the contact avatar instead of
//!   the app. Resolving it at send time means a branding change applies to the
//!   next notification without restarting the app.
//! * `image-path` is the sender's profile picture when WhatsApp supplies one
//!   through `opts.icon`. Quickshell renders it as the main avatar with the
//!   app icon as a small badge; dunst/mako display it as the notification
//!   image. If no avatar is available, the branding icon is used alone.
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

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use notify_rust::{Hint, Notification, NotificationResponse};
use tauri::{AppHandle, Emitter, Manager};

use crate::{app_state::AppState, assets};

pub fn send(
    app: AppHandle,
    id: String,
    title: String,
    body: String,
    icon_data_url: Option<String>,
) {
    tauri::async_runtime::spawn(async move {
        let (app_name, app_icon, avatar, desktop_entry) = {
            let state = app.state::<AppState>();
            (
                state.custom_app_name(),
                branding_icon(&state),
                icon_data_url
                    .as_deref()
                    .and_then(|data| cache_avatar(&state, data)),
                desktop_entry_id(),
            )
        };

        let mut notification = Notification::new();
        notification
            .appname(&app_name)
            .summary(&title)
            .body(&body)
            .action("default", "Open");

        if let Some(path) = app_icon.as_deref().and_then(Path::to_str) {
            // Small application logo. It also acts as the fallback when
            // WhatsApp did not supply a sender avatar.
            notification.icon(path);
        }
        if let Some(path) = avatar.as_deref().and_then(Path::to_str) {
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

/// Decode the `data:image/...;base64,...` URL produced by the renderer and
/// store it as the notification's main image. Reject non-image payloads so a
/// broken avatar does not replace the branding icon with an error placeholder.
fn cache_avatar(state: &AppState, data_url: &str) -> Option<PathBuf> {
    let (header, encoded) = data_url.split_once(',')?;
    let mime = header.strip_prefix("data:")?.split(';').next()?;
    if !mime.starts_with("image/") {
        return None;
    }
    let bytes = BASE64.decode(encoded.trim()).ok()?;
    if image::load_from_memory(&bytes).is_err() {
        return None;
    }

    let extension = match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        _ => "img",
    };
    cache_bytes(
        state,
        &bytes,
        format!("notification-avatar-{:016x}.{extension}", hash(&bytes)),
    )
}

/// Store the branding icon under a content-addressed name so changing the
/// branding produces a new path instead of leaving notification servers with a
/// cached copy of the old logo.
fn cache_icon(state: &AppState, bytes: &[u8]) -> Option<PathBuf> {
    cache_bytes(
        state,
        bytes,
        format!("notification-{:016x}.png", hash(bytes)),
    )
}

fn cache_bytes(state: &AppState, bytes: &[u8], name: String) -> Option<PathBuf> {
    let path = state.user_data.join("cache").join(name);

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok()?;
    }
    if !path.is_file() {
        fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
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

    fn test_state(dir: &Path) -> AppState {
        let config = whatszan_core::JsonConfig::new(dir.join("config.json"), Default::default());
        let persist =
            whatszan_core::JsonConfig::new(dir.join("persistent-state.json"), Default::default());
        AppState::new(dir.to_path_buf(), config, persist, false, false)
    }

    #[test]
    fn notification_icon_follows_branding_choice() {
        let dir = std::env::temp_dir().join(format!("whatszan-notif-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let state = test_state(&dir);

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

    #[test]
    fn avatar_data_url_is_cached_and_validated() {
        let dir = std::env::temp_dir().join(format!("whatszan-avatar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let state = test_state(&dir);

        let data_url = format!(
            "data:image/png;base64,{}",
            BASE64.encode(assets::WHATSAPP_ICON_PNG)
        );
        let path = cache_avatar(&state, &data_url).unwrap();
        assert!(path.is_file());
        assert!(image::load_from_memory(&fs::read(&path).unwrap()).is_ok());

        assert!(cache_avatar(&state, "data:text/plain;base64,aGVsbG8=").is_none());
        assert!(cache_avatar(&state, "data:image/png;base64,bm90IGFuIGltYWdl").is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
