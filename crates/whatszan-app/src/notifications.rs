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
//! SwayNotificationCenter implement this. Other servers (GNOME, Cinnamon,
//! MATE, XFCE, LXQt, dunst, mako, fnott) only support notification actions, so
//! the backend adds a "Reply" action that opens a small quick-reply window
//! (`notifications_reply_window.rs`).

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
const REPLY_ACTION_ID: &str = "reply";
const OPEN_CHAT_ACTION_ID: &str = "open-chat";

const REPLY_PLACEHOLDER: &str = "Reply to message...";
const REPLY_ACTION: &str = "Reply";
const REPLY_SEND: &str = "Send";
const REPLY_OPEN: &str = "Open chat";

/// Maps the notification server's id back to the renderer's message id while a
/// notification is waiting for a reply.
static PENDING_REPLIES: LazyLock<Mutex<HashMap<u32, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// `0` = not queried yet, `1` = supported, `2` = unsupported.
static INLINE_REPLY_SUPPORT: AtomicU8 = AtomicU8::new(0);

/// Cached `GetServerInformation` name. Only used to special-case mako, which
/// does not render action buttons and therefore needs the reply action to be
/// the notification's default action.
static SERVER_NAME: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

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
        let (placeholder, reply_action, reply_send, reply_open) = {
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
                translate(&translations, "notification_reply_open", REPLY_OPEN).to_string(),
            )
        };
        let inline_reply = !force_reply_fallback() && inline_reply_supported().await;
        // mako does not render action buttons; its only discoverable action is
        // the notification's default action (left click).
        let reply_is_default = !inline_reply && server_is_mako().await;

        let mut notification = Notification::new();
        notification.appname(&app_name).summary(&title).body(&body);

        if inline_reply {
            notification.action("default", "Open");
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
        } else if reply_is_default {
            notification.action("default", &reply_action);
            notification.action(OPEN_CHAT_ACTION_ID, &reply_open);
        } else {
            notification.action("default", "Open");
            notification.action(REPLY_ACTION_ID, &reply_action);
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
                tracing::debug!(server_id, message_id = %id, "desktop notification shown");
                if inline_reply {
                    PENDING_REPLIES
                        .lock()
                        .unwrap()
                        .insert(server_id, id.clone());
                }

                let reply_id = id.clone();
                let reply_title = title.clone();
                let reply_body = body.clone();
                handle
                    .wait_for_action_async(move |response| {
                        tracing::debug!(
                            ?response,
                            reply_is_default,
                            "desktop notification response"
                        );
                        if inline_reply {
                            PENDING_REPLIES.lock().unwrap().remove(&server_id);
                        }
                        if is_reply_response(response, reply_is_default) {
                            crate::notifications_reply_window::open(
                                &app,
                                reply_id.clone(),
                                reply_title.clone(),
                                reply_body.clone(),
                            );
                        } else if is_click_response(response) {
                            crate::webview::show_main_window(&app);
                            if let Some(window) = app.state::<AppState>().main_window() {
                                let _ = window.emit("notify-click", reply_id.clone());
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
/// Servers without the capability use the action fallback instead.
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

/// The KDE extension is the only in-toast reply mechanism. Allow a debug
/// override so the fallback window can be exercised on servers that do support
/// it (for example Quickshell during development).
fn force_reply_fallback() -> bool {
    std::env::var("WHATZAN_FORCE_REPLY_FALLBACK")
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// mako renders no action buttons, so its reply action has to be the default
/// action. Query the server name once and cache it.
async fn server_is_mako() -> bool {
    let cached = SERVER_NAME.lock().unwrap().clone();
    if let Some(name) = cached {
        return is_mako(&name);
    }

    let name = match notification_server_information().await {
        Ok((name, _, _, _)) => name,
        Err(err) => {
            tracing::debug!(%err, "cannot query notification server information");
            return false;
        }
    };
    *SERVER_NAME.lock().unwrap() = Some(name.clone());
    is_mako(&name)
}

fn is_mako(name: &str) -> bool {
    name.to_ascii_lowercase().contains("mako")
}

async fn notification_server_information() -> zbus::Result<(String, String, String, String)> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        NOTIFICATION_SERVICE,
        NOTIFICATION_PATH,
        NOTIFICATION_INTERFACE,
    )
    .await?;
    let reply = proxy.call_method("GetServerInformation", &()).await?;
    reply.body().deserialize()
}

/// Whether the response asks for the quick-reply window. `reply_is_default`
/// is true on mako, where the notification body click is the reply action.
fn is_reply_response(response: &NotificationResponse, reply_is_default: bool) -> bool {
    if reply_is_default {
        matches!(response, NotificationResponse::Default)
    } else {
        matches!(response, NotificationResponse::Action(action) if action == REPLY_ACTION_ID)
    }
}

/// A reply submission must not also count as a body click; otherwise the chat
/// would be opened twice and the paste could race with the window switch.
fn is_click_response(response: &NotificationResponse) -> bool {
    match response {
        NotificationResponse::Default => true,
        NotificationResponse::Action(action) => {
            action != INLINE_REPLY_ACTION && action != REPLY_ACTION_ID
        }
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
        assert!(is_click_response(&NotificationResponse::Action(
            OPEN_CHAT_ACTION_ID.into()
        )));
        assert!(!is_click_response(&NotificationResponse::Action(
            INLINE_REPLY_ACTION.into()
        )));
        assert!(!is_click_response(&NotificationResponse::Action(
            REPLY_ACTION_ID.into()
        )));
        assert!(!is_click_response(&NotificationResponse::Closed(
            CloseReason::Dismissed
        )));
    }

    #[test]
    fn reply_responses_are_separate_from_clicks() {
        assert!(is_reply_response(
            &NotificationResponse::Action(REPLY_ACTION_ID.into()),
            false
        ));
        assert!(!is_reply_response(&NotificationResponse::Default, false));
        assert!(is_reply_response(&NotificationResponse::Default, true));
        assert!(!is_reply_response(
            &NotificationResponse::Action(OPEN_CHAT_ACTION_ID.into()),
            true
        ));
    }

    #[test]
    fn mako_is_detected_case_insensitively() {
        assert!(is_mako("mako"));
        assert!(is_mako("Mako"));
        assert!(!is_mako("dunst"));
    }
}
