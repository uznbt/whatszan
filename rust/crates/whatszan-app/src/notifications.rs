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

use std::path::Path;

use notify_rust::{Hint, Notification, NotificationResponse};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    app_state::AppState,
    notifications_common::{branding_icon, cache_avatar},
};

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
