//! Windows system tray icon implemented with Tauri's `tray-icon` backend.
//!
//! The menu mirrors the Linux StatusNotifierItem tray: show/hide, settings,
//! Do Not Disturb, and quit. Unread state swaps the icon for the branded
//! "unread" artwork or draws a red badge over the normal icon.

use std::{path::Path, sync::atomic::Ordering};

use image::{Rgba, RgbaImage};
use serde_json::json;
use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuEvent},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use whatszan_core::translations::t as translate;

use crate::{
    app_state::AppState,
    assets,
    webview::{open_settings, set_dnd_mute, toggle_main_window},
};

const TRAY_ID: &str = "whatszan";

pub fn spawn(app: AppHandle) {
    let state = app.state::<AppState>();
    let menu = match build_menu(&app) {
        Ok(menu) => menu,
        Err(err) => {
            tracing::warn!(%err, "cannot build Windows tray menu");
            return;
        }
    };
    let app_name = state.custom_app_name();
    let unread = state.unread.load(Ordering::Relaxed);
    let icon = tray_image(&state, unread);

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(&app_name)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = toggle_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    match builder.build(&app) {
        Ok(tray) => {
            *state.tray.lock().unwrap() = Some(tray);
            tracing::info!("Windows tray icon registered");
        }
        Err(err) => tracing::warn!(%err, "cannot register Windows tray icon"),
    }
}

pub async fn set_unread(app: &AppHandle, unread: u32) {
    let state = app.state::<AppState>();
    let tray = { state.tray.lock().unwrap().clone() };
    let Some(tray) = tray else {
        return;
    };
    if let Some(icon) = tray_image(&state, unread) {
        let _ = tray.set_icon(Some(icon));
    }
}

/// Re-read custom icon paths, translations, and the DND state after the
/// settings window changes branding or language.
pub async fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let tray = { state.tray.lock().unwrap().clone() };
    let Some(tray) = tray else {
        return;
    };

    if let Ok(menu) = build_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
    let unread = state.unread.load(Ordering::Relaxed);
    if let Some(icon) = tray_image(&state, unread) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(state.custom_app_name()));
}

fn build_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let translations = state.translations();
    let dnd = CheckMenuItem::with_id(
        app,
        "tray-dnd",
        translate(&translations, "dnd_mode", "Mode Jangan Ganggu (DND)"),
        true,
        state.is_dnd(),
        None::<&str>,
    )?;

    MenuBuilder::new(app)
        .text(
            "tray-show-hide",
            translate(&translations, "show_hide", "Show/Hide"),
        )
        .text(
            "tray-settings",
            translate(&translations, "settings", "Pengaturan..."),
        )
        .separator()
        .item(&dnd)
        .separator()
        .text("tray-quit", translate(&translations, "quit", "Quit"))
        .build()
}

fn handle_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "tray-show-hide" => {
            let _ = toggle_main_window(app);
        }
        "tray-settings" => open_settings(app),
        "tray-dnd" => toggle_dnd(app),
        "tray-quit" => {
            if let Some(state) = app.try_state::<AppState>() {
                state.set_quitting(true);
            }
            app.exit(0);
        }
        _ => {}
    }
}

fn toggle_dnd(app: &AppHandle) {
    let state = app.state::<AppState>();
    let next = !state.is_dnd();
    state.set_dnd(next);
    {
        let mut persist = state.persist.write().unwrap();
        let _ = persist.set("dnd", json!(next));
    }
    if let Some(window) = state.main_window() {
        set_dnd_mute(&window, next);
    }

    let tray = { state.tray.lock().unwrap().clone() };
    if let Some(tray) = tray {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    tracing::info!(enabled = next, "[DND] state changed");
}

fn tray_image(state: &AppState, unread: u32) -> Option<tauri::image::Image<'static>> {
    let (normal, unread_icon) = {
        let persist = state.persist.read().unwrap();
        (
            non_empty(persist.get_str("custom_tray_normal", "")),
            non_empty(persist.get_str("custom_tray_unread", "")),
        )
    };

    if unread > 0 {
        if let Some(path) = unread_icon.filter(|path| Path::new(path).is_file()) {
            return crate::webview::load_tauri_icon(Path::new(&path));
        }
    }

    let base = normal
        .filter(|path| Path::new(path).is_file())
        .and_then(|path| crate::webview::load_tauri_icon(Path::new(&path)))
        .or_else(|| crate::webview::load_tauri_icon_bytes(assets::APP_ICON_PNG))?;

    if unread == 0 {
        Some(base)
    } else {
        Some(with_badge(base))
    }
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn with_badge(image: tauri::image::Image<'static>) -> tauri::image::Image<'static> {
    let (width, height) = (image.width(), image.height());
    let Some(mut rgba) = RgbaImage::from_raw(width, height, image.rgba().to_vec()) else {
        return image;
    };
    draw_badge(&mut rgba);
    tauri::image::Image::new_owned(rgba.into_raw(), width, height)
}

fn draw_badge(image: &mut RgbaImage) {
    let size = image.width() as f32;
    let radius = (size * 0.20).max(6.0);
    let border = (radius * 0.20).max(1.5);
    let cx = size - radius - 2.0;
    let cy = radius + 2.0;

    for y in 0..image.height() {
        for x in 0..image.width() {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance <= radius {
                image.put_pixel(x, y, Rgba([220, 38, 38, 255]));
            } else if distance <= radius + border {
                image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }
}
