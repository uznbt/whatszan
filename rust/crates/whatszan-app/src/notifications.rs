//! Native desktop notifications.
//!
//! The Electron build used `new Notification()` from Electron. On Linux the
//! same thing is an XDG desktop notification; `notify-rust` speaks that
//! protocol directly. Clicking the notification raises the main window and
//! forwards the message id to the renderer so it can open the chat.
//!
//! Inline replies are a macOS-only concept in `notify-rust`; XDG
//! notifications have no portable "reply text" action, so `notify-reply` is
//! intentionally not emitted here.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use notify_rust::{Notification, NotificationResponse};
use tauri::{AppHandle, Emitter, Manager};

use crate::app_state::AppState;

pub fn send(
    app: AppHandle,
    id: String,
    title: String,
    body: String,
    icon_data_url: Option<String>,
) {
    tauri::async_runtime::spawn(async move {
        let app_name = {
            let state = app.state::<AppState>();
            state.custom_app_name()
        };

        let mut notification = Notification::new();
        notification
            .appname(&app_name)
            .summary(&title)
            .body(&body)
            .action("default", "Open");

        if let Some(path) = icon_data_url
            .as_deref()
            .and_then(|data| write_icon(&app, data))
        {
            if let Some(path) = path.to_str() {
                notification.icon(path);
            }
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

/// Decode a `data:image/...;base64,...` URL and store it in the cache folder.
/// XDG notifications accept an icon path, not an in-memory image.
fn write_icon(app: &AppHandle, data_url: &str) -> Option<PathBuf> {
    let (prefix, encoded) = data_url.split_once(',')?;
    if !prefix.starts_with("data:image/") {
        return None;
    }
    let bytes = BASE64.decode(encoded).ok()?;

    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let name = format!("notification-{:016x}.png", hasher.finish());

    let state = app.state::<AppState>();
    let dir = state.user_data.join("cache");
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(name);
    fs::write(&path, bytes).ok()?;
    Some(path)
}
