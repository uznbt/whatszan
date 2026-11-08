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
//! Inline replies use the KDE extension to the XDG protocol: the server
//! advertises the `inline-reply` capability, the notification carries an
//! `inline-reply` action plus `x-kde-reply-*` hints, and the submitted text
//! arrives through the `NotificationReplied` signal. KDE Plasma 5.18+ and
//! SwayNotificationCenter implement this; GNOME Shell does not, so the action
//! is only added when the running server advertises support.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicU8, Ordering},
        LazyLock, Mutex,
    },
};

use futures_util::StreamExt;
use notify_rust::{Hint, Notification, NotificationResponse};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use whatszan_core::translations::t as translate;

use crate::{
    app_state::AppState,
    notifications_common::{branding_icon, cache_avatar},
};

const NOTIFICATION_SERVICE: &str = "org.freedesktop.Notifications";
const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_INTERFACE: &str = "org.freedesktop.Notifications";
const INLINE_REPLY_ACTION: &str = "inline-reply";

const REPLY_PLACEHOLDER: &str = "Reply to message...";
const REPLY_ACTION: &str = "Reply";
const REPLY_SEND: &str = "Send";

/// Maps the notification server's id back to the renderer's message id while a
/// notification is waiting for a reply.
static PENDING_REPLIES: LazyLock<Mutex<HashMap<u32, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// `0` = not queried yet, `1` = supported, `2` = unsupported.
static INLINE_REPLY_SUPPORT: AtomicU8 = AtomicU8::new(0);

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
        let (placeholder, reply_action, reply_send) = {
            let state = app.state::<AppState>();
            let translations = state.translations();
            (
                translate(
                    &translations,
                    "notification_reply_placeholder",
                    REPLY_PLACEHOLDER,
                )
                .to_string(),
                translate(&translations, "notification_reply_action", REPLY_ACTION).to_string(),
                translate(&translations, "notification_reply_send", REPLY_SEND).to_string(),
            )
        };
        let inline_reply = inline_reply_supported().await;

        let mut notification = Notification::new();
        notification
            .appname(&app_name)
            .summary(&title)
            .body(&body)
            .action("default", "Open");

        if inline_reply {
            notification
                .action(INLINE_REPLY_ACTION, &reply_action)
                .hint(Hint::Custom(
                    "x-kde-reply-placeholder-text".into(),
                    placeholder,
                ))
                .hint(Hint::Custom(
                    "x-kde-reply-submit-button-text".into(),
                    reply_send,
                ));
        }

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
                let server_id = handle.id();
                if inline_reply {
                    PENDING_REPLIES
                        .lock()
                        .unwrap()
                        .insert(server_id, id.clone());
                }

                handle
                    .wait_for_action_async(move |response| {
                        if inline_reply {
                            PENDING_REPLIES.lock().unwrap().remove(&server_id);
                        }
                        if is_click_response(response) {
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

/// Subscribe to `NotificationReplied` once and route replies to the renderer.
///
/// This signal is a KDE extension that SwayNotificationCenter also implements.
/// Servers without the capability are left alone so GNOME does not show a
/// useless "Reply" button that only opens the chat.
pub async fn start_reply_listener(app: AppHandle) {
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(err) => {
            tracing::debug!(%err, "cannot connect to the session bus for notification replies");
            return;
        }
    };
    let proxy = match zbus::Proxy::new(
        &connection,
        NOTIFICATION_SERVICE,
        NOTIFICATION_PATH,
        NOTIFICATION_INTERFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(err) => {
            tracing::debug!(%err, "cannot create the notification proxy");
            return;
        }
    };

    let supported = match proxy.call_method("GetCapabilities", &()).await {
        Ok(reply) => reply
            .body()
            .deserialize::<(Vec<String>,)>()
            .map(|(capabilities,)| {
                capabilities
                    .iter()
                    .any(|capability| capability == INLINE_REPLY_ACTION)
            })
            .unwrap_or(false),
        Err(err) => {
            tracing::debug!(%err, "cannot query notification capabilities");
            false
        }
    };
    INLINE_REPLY_SUPPORT.store(if supported { 1 } else { 2 }, Ordering::Relaxed);
    if !supported {
        return;
    }

    let mut replies = match proxy.receive_signal("NotificationReplied").await {
        Ok(stream) => stream,
        Err(err) => {
            tracing::warn!(%err, "cannot listen for notification replies");
            return;
        }
    };

    while let Some(message) = replies.next().await {
        let Ok((server_id, text)) = message.body().deserialize::<(u32, String)>() else {
            tracing::warn!("cannot decode the NotificationReplied signal");
            continue;
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let Some(message_id) = PENDING_REPLIES.lock().unwrap().remove(&server_id) else {
            continue;
        };

        crate::webview::show_main_window(&app);
        if let Some(window) = app.state::<AppState>().main_window() {
            let _ = window.emit("notify-reply", json!([message_id, text]));
        }
    }
}

/// Whether the notification server advertises the KDE inline-reply extension.
async fn inline_reply_supported() -> bool {
    match INLINE_REPLY_SUPPORT.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }

    let supported = match notification_capabilities().await {
        Ok(capabilities) => capabilities
            .iter()
            .any(|capability| capability == INLINE_REPLY_ACTION),
        Err(err) => {
            tracing::debug!(%err, "cannot query notification capabilities");
            false
        }
    };
    INLINE_REPLY_SUPPORT.store(if supported { 1 } else { 2 }, Ordering::Relaxed);
    supported
}

async fn notification_capabilities() -> zbus::Result<Vec<String>> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        NOTIFICATION_SERVICE,
        NOTIFICATION_PATH,
        NOTIFICATION_INTERFACE,
    )
    .await?;
    let reply = proxy.call_method("GetCapabilities", &()).await?;
    let (capabilities,): (Vec<String>,) = reply.body().deserialize()?;
    Ok(capabilities)
}

/// A reply submission must not also count as a body click; otherwise the chat
/// would be opened twice and the paste could race with the window switch.
fn is_click_response(response: &NotificationResponse) -> bool {
    match response {
        NotificationResponse::Default => true,
        NotificationResponse::Action(action) => action != INLINE_REPLY_ACTION,
        _ => false,
    }
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
    use notify_rust::CloseReason;

    #[test]
    fn only_real_clicks_open_the_chat() {
        assert!(is_click_response(&NotificationResponse::Default));
        assert!(is_click_response(&NotificationResponse::Action(
            "default".into()
        )));
        assert!(!is_click_response(&NotificationResponse::Action(
            INLINE_REPLY_ACTION.into()
        )));
        assert!(!is_click_response(&NotificationResponse::Closed(
            CloseReason::Dismissed
        )));
    }
}
