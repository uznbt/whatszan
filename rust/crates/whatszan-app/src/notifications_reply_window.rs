//! Small quick-reply window for notification servers without inline replies.
//!
//! GNOME, Cinnamon, MATE, XFCE, LXQt, dunst, mako, and fnott do not implement
//! the KDE `inline-reply` extension. They do support notification actions,
//! though, so the Linux backend adds a "Reply" action when the capability is
//! missing. Activating that action opens this window; the submitted text goes
//! back through the renderer's existing `notify-reply` path, which opens the
//! chat, pastes the reply, and sends it.
//!
//! The window is reused between notifications: opening a reply for another
//! message updates the sender/body preview and clears the previous context.

use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use whatszan_core::translations::t as translate;

use crate::{app_state::AppState, webview};

const LABEL: &str = "notification-reply";
const REPLY_PLACEHOLDER: &str = "Reply to message...";
const REPLY_ACTION: &str = "Reply";
const REPLY_SEND: &str = "Send";
const REPLY_OPEN: &str = "Open chat";

/// Data shown by `reply.html`. `message_id` stays in the Rust process; the
/// page only needs the labels and the preview.
#[derive(Clone, Serialize)]
pub struct ReplyContext {
    pub title: String,
    pub body: String,
    pub placeholder: String,
    pub send_label: String,
    pub open_label: String,
    #[serde(skip)]
    pub message_id: String,
}

static CONTEXT: LazyLock<Mutex<Option<ReplyContext>>> = LazyLock::new(|| Mutex::new(None));

/// Open (or focus) the quick-reply window for a notification.
pub fn open(app: &AppHandle, message_id: String, title: String, body: String) {
    tracing::debug!(%message_id, "opening notification reply window");
    let state = app.state::<AppState>();
    let translations = state.translations();
    let context = ReplyContext {
        title,
        body,
        placeholder: translate(
            &translations,
            "notification_reply_placeholder",
            REPLY_PLACEHOLDER,
        )
        .to_string(),
        send_label: translate(&translations, "notification_reply_send", REPLY_SEND).to_string(),
        open_label: translate(&translations, "notification_reply_open", REPLY_OPEN).to_string(),
        message_id,
    };
    let window_title = format!(
        "{} — {}",
        translate(&translations, "notification_reply_action", REPLY_ACTION),
        state.custom_app_name()
    );

    *CONTEXT.lock().unwrap() = Some(context.clone());

    let scheduled = app.clone();
    if let Err(err) = app.run_on_main_thread(move || {
        present_or_create(&scheduled, context, window_title);
    }) {
        tracing::warn!(%err, "cannot schedule notification reply window");
    }
}

fn present_or_create(app: &AppHandle, context: ReplyContext, window_title: String) {
    tracing::debug!(label = LABEL, "presenting notification reply window");
    if let Some(window) = app.get_webview_window(LABEL) {
        let context_json = serde_json::to_string(&context).unwrap_or_else(|_| "null".into());
        let _ = window.eval(format!(
            "window.__wzReplyContext && window.__wzReplyContext({context_json});"
        ));
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let state = app.state::<AppState>();
    let dark = state.is_theme_dark();
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("reply.html".into()))
        .title(window_title)
        .inner_size(420.0, 240.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .theme(Some(if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }))
        .background_color(if dark {
            tauri::webview::Color(22, 23, 23, 255)
        } else {
            tauri::webview::Color(248, 245, 243, 255)
        })
        .initialization_script(if dark {
            "document.documentElement.classList.add('dark');"
        } else {
            ""
        });

    // A transient-for window is treated as a dialog by tiling compositors
    // (Hyprland, Sway), so it floats at the requested size instead of being
    // tiled to the full height of the workspace.
    let builder = if let Some(main) = webview::main_window(app) {
        match builder.parent(&main) {
            Ok(builder) => builder,
            Err(err) => {
                tracing::warn!(%err, "cannot set notification reply window parent");
                return;
            }
        }
    } else {
        builder
    };

    let window = match builder.build() {
        Ok(window) => window,
        Err(err) => {
            tracing::warn!(%err, "cannot create notification reply window");
            return;
        }
    };
    webview::set_window_icon(&window);
    let _ = window.set_focus();
}

#[tauri::command]
pub fn notification_reply_context() -> Option<ReplyContext> {
    tracing::debug!("notification_reply_context requested");
    CONTEXT.lock().unwrap().clone()
}

#[tauri::command]
pub fn notification_reply_submit(app: AppHandle, window: tauri::WebviewWindow, text: String) {
    tracing::debug!(
        label = window.label(),
        len = text.len(),
        "notification_reply_submit"
    );
    if window.label() != LABEL {
        return;
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let Some(context) = CONTEXT.lock().unwrap().take() else {
        return;
    };
    let _ = window.close();
    webview::show_main_window(&app);
    if let Some(main) = webview::main_window(&app) {
        let _ = main.emit("notify-reply", json!([context.message_id, text]));
    }
}

#[tauri::command]
pub fn notification_reply_open_chat(app: AppHandle, window: tauri::WebviewWindow) {
    tracing::debug!(label = window.label(), "notification_reply_open_chat");
    if window.label() != LABEL {
        return;
    }
    let Some(context) = CONTEXT.lock().unwrap().take() else {
        return;
    };
    let _ = window.close();
    webview::show_main_window(&app);
    if let Some(main) = webview::main_window(&app) {
        let _ = main.emit("notify-click", context.message_id);
    }
}

#[tauri::command]
pub fn notification_reply_cancel(window: tauri::WebviewWindow) {
    tracing::debug!(label = window.label(), "notification_reply_cancel");
    if window.label() != LABEL {
        return;
    }
    let _ = window.close();
}
