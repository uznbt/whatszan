//! Application menu.
//!
//! Electron's default menu is recreated with Tauri's native menu API. The
//! settings/restore entries are appended to a Window menu there; here they
//! live in the application menu, which is the closest GTK equivalent.
//!
//! `menu-bar-auto-hide` cannot be implemented as a true GTK auto-hide menu
//! (GTK has no equivalent), so the menu is hidden at startup and the bridge
//! toggles it with the Alt key.

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Manager, WebviewWindow,
};
use whatszan_core::translations::t;

use crate::{app_state::AppState, webview};

static ZOOM: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32

pub fn apply_menu(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<AppState>();
    let (show_menu, auto_hide, translations, app_name) = {
        let config = state.config.read().unwrap();
        (
            config.get_bool("menu-bar", true),
            config.get_bool("menu-bar-auto-hide", true),
            state.translations(),
            state.custom_app_name(),
        )
    };

    if !show_menu {
        let _ = window.remove_menu();
        return;
    }

    let settings_label = t(&translations, "settings", "Settings...");
    let quit_label = t(&translations, "quit", "Quit");

    let menu = (|| -> tauri::Result<_> {
        let about = MenuItemBuilder::with_id("about", format!("About {app_name}")).build(app)?;
        let settings = MenuItemBuilder::with_id("settings", settings_label)
            .accelerator("CmdOrCtrl+,")
            .build(app)?;
        let quit = MenuItemBuilder::with_id("quit", quit_label)
            .accelerator("CmdOrCtrl+Q")
            .build(app)?;
        let app_menu = SubmenuBuilder::new(app, &app_name)
            .item(&about)
            .separator()
            .item(&settings)
            .separator()
            .item(&quit)
            .build()?;

        let undo = MenuItemBuilder::with_id("undo", "Undo")
            .accelerator("CmdOrCtrl+Z")
            .build(app)?;
        let redo = MenuItemBuilder::with_id("redo", "Redo")
            .accelerator("CmdOrCtrl+Shift+Z")
            .build(app)?;
        let cut = MenuItemBuilder::with_id("cut", "Cut")
            .accelerator("CmdOrCtrl+X")
            .build(app)?;
        let copy = MenuItemBuilder::with_id("copy", "Copy")
            .accelerator("CmdOrCtrl+C")
            .build(app)?;
        let paste = MenuItemBuilder::with_id("paste", "Paste")
            .accelerator("CmdOrCtrl+V")
            .build(app)?;
        let select_all = MenuItemBuilder::with_id("select-all", "Select All")
            .accelerator("CmdOrCtrl+A")
            .build(app)?;
        let edit_menu = SubmenuBuilder::new(app, "Edit")
            .item(&undo)
            .item(&redo)
            .separator()
            .item(&cut)
            .item(&copy)
            .item(&paste)
            .item(&select_all)
            .build()?;

        let reload = MenuItemBuilder::with_id("reload", "Reload")
            .accelerator("CmdOrCtrl+R")
            .build(app)?;
        let devtools = MenuItemBuilder::with_id("toggle-devtools", "Toggle Developer Tools")
            .accelerator("CmdOrCtrl+Shift+I")
            .build(app)?;
        let actual_size = MenuItemBuilder::with_id("zoom-reset", "Actual Size")
            .accelerator("CmdOrCtrl+0")
            .build(app)?;
        let zoom_in = MenuItemBuilder::with_id("zoom-in", "Zoom In")
            .accelerator("CmdOrCtrl+Plus")
            .build(app)?;
        let zoom_out = MenuItemBuilder::with_id("zoom-out", "Zoom Out")
            .accelerator("CmdOrCtrl+-")
            .build(app)?;
        let fullscreen = MenuItemBuilder::with_id("fullscreen", "Fullscreen")
            .accelerator("F11")
            .build(app)?;
        let view_menu = SubmenuBuilder::new(app, "View")
            .item(&reload)
            .item(&devtools)
            .separator()
            .item(&actual_size)
            .item(&zoom_in)
            .item(&zoom_out)
            .separator()
            .item(&fullscreen)
            .build()?;

        let minimize = MenuItemBuilder::with_id("minimize", "Minimize")
            .accelerator("CmdOrCtrl+M")
            .build(app)?;
        let hide = MenuItemBuilder::with_id("hide-window", "Hide")
            .accelerator("CmdOrCtrl+H")
            .build(app)?;
        let restore =
            MenuItemBuilder::with_id("restore-session", "Restore WhatsApp Session from Backup")
                .build(app)?;
        let window_menu = SubmenuBuilder::new(app, "Window")
            .item(&minimize)
            .item(&hide)
            .separator()
            .item(&restore)
            .build()?;

        let homepage = MenuItemBuilder::with_id("homepage", "Project Homepage").build(app)?;
        let help_menu = SubmenuBuilder::new(app, "Help").item(&homepage).build()?;

        MenuBuilder::new(app)
            .item(&app_menu)
            .item(&edit_menu)
            .item(&view_menu)
            .item(&window_menu)
            .item(&help_menu)
            .build()
    })();

    match menu {
        Ok(menu) => {
            if let Err(err) = window.set_menu(menu) {
                tracing::warn!(%err, "cannot set application menu");
                return;
            }
            if auto_hide {
                let _ = window.hide_menu();
            } else {
                let _ = window.show_menu();
            }
        }
        Err(err) => tracing::warn!(%err, "cannot build application menu"),
    }
}

pub fn toggle_menu(app: &AppHandle) {
    let Some(window) = webview::main_window(app) else {
        return;
    };
    let visible = window.is_menu_visible().unwrap_or(false);
    if visible {
        let _ = window.hide_menu();
    } else {
        let _ = window.show_menu();
    }
}

pub fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "settings" => webview::open_settings(app),
        "restore-session" => restore_session(app),
        "quit" => {
            if let Some(state) = app.try_state::<AppState>() {
                state.set_quitting(true);
            }
            app.exit(0);
        }
        "about" => show_about(app),
        "homepage" => {
            let _ = open::that("https://github.com/uznbt/whatszan");
        }
        "reload" => {
            if let Some(window) = webview::main_window(app) {
                let _ = window.eval("location.reload()");
            }
        }
        "toggle-devtools" => {
            if let Some(window) = webview::main_window(app) {
                if window.is_devtools_open() {
                    window.close_devtools();
                } else {
                    window.open_devtools();
                }
            }
        }
        "zoom-in" => set_zoom(app, 1.1),
        "zoom-out" => set_zoom(app, 1.0 / 1.1),
        "zoom-reset" => {
            ZOOM.store(0x3f80_0000, Ordering::Relaxed);
            if let Some(window) = webview::main_window(app) {
                let _ = window.set_zoom(1.0);
            }
        }
        "fullscreen" => {
            if let Some(window) = webview::main_window(app) {
                let fullscreen = window.is_fullscreen().unwrap_or(false);
                let _ = window.set_fullscreen(!fullscreen);
            }
        }
        "minimize" => {
            if let Some(window) = webview::main_window(app) {
                let _ = window.minimize();
            }
        }
        "hide-window" => webview::hide_main_window(app),
        "undo" => edit_command(app, "Undo"),
        "redo" => edit_command(app, "Redo"),
        "cut" => edit_command(app, "Cut"),
        "copy" => edit_command(app, "Copy"),
        "paste" => edit_command(app, "Paste"),
        "select-all" => edit_command(app, "SelectAll"),
        _ => {}
    }
}

fn set_zoom(app: &AppHandle, multiplier: f32) {
    let Some(window) = webview::main_window(app) else {
        return;
    };
    let current = f32::from_bits(ZOOM.load(Ordering::Relaxed));
    let next = (current * multiplier).clamp(0.5, 3.0);
    ZOOM.store(next.to_bits(), Ordering::Relaxed);
    let _ = window.set_zoom(next as f64);
}

fn edit_command(app: &AppHandle, command: &'static str) {
    let Some(window) = webview::main_window(app) else {
        return;
    };

    #[cfg(target_os = "linux")]
    {
        let _ = window.with_webview(move |webview| {
            use webkit2gtk::WebViewExt;
            webview.inner().execute_editing_command(command);
        });
    }

    #[cfg(not(target_os = "linux"))]
    {
        let command = command.to_ascii_lowercase();
        let _ = window.eval(format!(
            "try {{ document.execCommand('{}', false, null); }} catch (e) {{}}",
            if command == "selectall" {
                "selectAll"
            } else {
                &command
            }
        ));
    }
}

fn show_about(app: &AppHandle) {
    let name = app
        .try_state::<AppState>()
        .map(|state| state.custom_app_name())
        .unwrap_or_else(|| "WhatsZan".to_string());
    let version = env!("CARGO_PKG_VERSION");
    let platform = if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    };
    let description = format!(
        "{name} {version}\n\nUnofficial WhatsApp desktop client for {platform}.\n\
         Rust/Tauri rewrite of the original Electron application."
    );
    tauri::async_runtime::spawn(async move {
        crate::dialogs::message(
            crate::dialogs::Level::Info,
            format!("About {name}"),
            description,
            crate::dialogs::Buttons::ok("OK"),
        )
        .await;
    });
}

fn restore_session(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let user_data = state.user_data.clone();
    let snapshot = match whatszan_core::session_backup::request_session_restore(&user_data) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            tauri::async_runtime::spawn(async move {
                crate::dialogs::message(
                    crate::dialogs::Level::Info,
                    "Cadangan sesi",
                    "Belum ada cadangan sesi.\n\nCadangan dibuat otomatis setiap kali \
                     WhatsZan dijalankan.",
                    crate::dialogs::Buttons::ok("OK"),
                )
                .await;
            });
            return;
        }
        Err(err) => {
            tracing::warn!(%err, "cannot request session restore");
            return;
        }
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let restore = crate::dialogs::message(
            crate::dialogs::Level::Warning,
            "Pulihkan sesi WhatsApp",
            format!(
                "Pulihkan sesi dari cadangan?\n\nCadangan: {snapshot}\n\n\
                 WhatsZan akan ditutup lalu dibuka kembali secara otomatis.\n\n\
                 Gunakan ini hanya kalau WhatsApp meminta scan QR padahal sebelumnya \
                 sudah login — biasanya setelah aplikasi tertutup paksa atau crash."
            ),
            crate::dialogs::Buttons::yes_no("Ya", "Tidak"),
        )
        .await;

        if restore {
            if let Some(state) = app.try_state::<AppState>() {
                state.set_quitting(true);
            }
            app.restart();
        }
    });
}
