//! Native Windows toast notifications.
//!
//! Electron's renderer used the HTML5 `Notification` API, which Chromium turns
//! into a Windows toast. The Rust build receives the same call from the DOM
//! shim and forwards it to WinRT directly through `tauri-winrt-notification`.
//!
//! The toast is registered under the process AUMID. `shell_windows` creates the
//! Start Menu shortcut that Windows requires for an unpackaged desktop app to
//! show notifications under its own identity. Clicking a toast raises the main
//! window and forwards the message id to the renderer, exactly like the Linux
//! backend.

use tauri::{AppHandle, Emitter, Manager};
use tauri_winrt_notification::{IconCrop, Toast};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

use crate::{app_state::AppState, notifications_common, shell_windows::APP_ID, webview};

/// Show a toast for a WhatsApp message.
///
/// `icon_data_url` is the sender avatar supplied by the renderer. When it is
/// missing or invalid the current branding icon is used instead, matching the
/// Linux notification backend.
pub fn send(
    app: AppHandle,
    id: String,
    title: String,
    body: String,
    icon_data_url: Option<String>,
) {
    let state = app.state::<AppState>();
    let avatar = icon_data_url
        .as_deref()
        .and_then(|data_url| notifications_common::cache_avatar(&state, data_url));
    let branding = notifications_common::branding_icon(&state);
    let is_avatar = avatar.is_some();
    let image = avatar.or(branding);

    // Tauri commands may run on a thread that has no COM apartment. WinRT
    // needs one; a failed `RoInitialize` usually means the thread is already
    // initialized (possibly as STA by the window code), which is fine too.
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();

    let click_app = app.clone();
    let click_id = id.clone();
    let mut toast = Toast::new(APP_ID).title(&title).text1(&body);
    if let Some(path) = image {
        let crop = if is_avatar {
            IconCrop::Circular
        } else {
            IconCrop::Square
        };
        toast = toast.icon(&path, crop, "WhatsZan");
    }

    let result = toast
        .on_activated(move |_action| {
            webview::show_main_window(&click_app);
            if let Some(window) = click_app
                .try_state::<AppState>()
                .and_then(|state| state.main_window())
            {
                let _ = window.emit("notify-click", click_id.clone());
            }
            Ok(())
        })
        .show();

    if let Err(err) = result {
        tracing::warn!(%err, %title, "cannot show Windows toast notification");
    }
    if initialized {
        unsafe { RoUninitialize() };
    }
}
