//! D-Bus service and Unity LauncherEntry badge.
//!
//! The Electron build exposed `org.uznbt.whatszan` on the session bus so
//! launchers and scripts can show/hide/toggle/quit the window. It also emitted
//! `com.canonical.Unity.LauncherEntry.Update` to update the dock badge on
//! GNOME (dash-to-dock), KDE Plasma, and Unity.

use std::collections::HashMap;

use tauri::{AppHandle, Manager};
use zbus::{interface, Connection, Variant};

use crate::{app_state::AppState, webview};

pub const SERVICE_NAME: &str = "org.uznbt.whatszan";
pub const OBJECT_PATH: &str = "/org/uznbt/whatszan";

struct WhatsZanService {
    app: AppHandle,
}

#[interface(name = "org.uznbt.whatszan")]
impl WhatsZanService {
    fn show(&self) {
        webview::show_main_window(&self.app);
    }

    fn hide(&self) {
        webview::hide_main_window(&self.app);
    }

    fn visible(&self) -> bool {
        webview::main_window(&self.app)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
    }

    fn toggle_visibility(&self) -> bool {
        webview::toggle_main_window(&self.app)
    }

    fn quit(&self) {
        if let Some(state) = self.app.try_state::<AppState>() {
            state.set_quitting(true);
        }
        self.app.exit(0);
    }
}

/// Register the service on the session bus. Errors are non-fatal: the client
/// still works without D-Bus integration.
pub async fn start(app: AppHandle) {
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(err) => {
            tracing::warn!(%err, "cannot connect to the session bus");
            return;
        }
    };

    let service = WhatsZanService { app: app.clone() };
    match connection.object_server().at(OBJECT_PATH, service).await {
        Ok(None) => {}
        Ok(Some(_)) => {
            tracing::warn!("D-Bus object path already occupied");
            return;
        }
        Err(err) => {
            tracing::warn!(%err, "cannot export D-Bus interface");
            return;
        }
    }

    if let Err(err) = connection.request_name(SERVICE_NAME).await {
        tracing::warn!(%err, "cannot own D-Bus name");
        return;
    }

    if let Some(state) = app.try_state::<AppState>() {
        *state.dbus.lock().unwrap() = Some(connection);
    }
    tracing::info!("D-Bus service registered: {SERVICE_NAME}");
}

/// Emit the Unity LauncherEntry badge update. This is what Electron's
/// `app.setBadgeCount()` ultimately does on Linux.
pub async fn set_badge_count(app: &AppHandle, unread: u32) {
    let connection = {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let connection = state.dbus.lock().unwrap().clone();
        connection
    };
    let Some(connection) = connection else {
        return;
    };

    let desktop_file = std::env::var("BAMF_DESKTOP_FILE_HINT")
        .ok()
        .and_then(|hint| {
            std::path::Path::new(&hint)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| std::env::var_os("SNAP").map(|_| "whatszan_whatszan.desktop".to_string()))
        .unwrap_or_else(|| "whatszan.desktop".to_string());
    let application = format!("application://{desktop_file}");

    let mut properties: HashMap<&str, Variant> = HashMap::new();
    properties.insert("count", Variant::I64(unread as i64));
    properties.insert("count-visible", Variant::Bool(unread != 0));

    if let Err(err) = connection
        .emit_signal(
            None::<&str>,
            "/",
            "com.canonical.Unity.LauncherEntry",
            "Update",
            &(application, properties),
        )
        .await
    {
        tracing::debug!(%err, "cannot emit launcher badge signal");
    }
}
