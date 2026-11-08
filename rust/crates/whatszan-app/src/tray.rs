//! StatusNotifierItem tray implemented in pure Rust with `ksni`.
//!
//! The Electron build used `Tray`/`Menu` from Electron. `ksni` speaks the
//! same D-Bus protocol directly, so no libappindicator/GTK dependency is
//! needed and the tray works on KDE, Hyprland (with an indicator extension),
//! and GNOME (AppIndicator extension).

use std::path::{Path, PathBuf};

use image::{imageops::FilterType, Rgba, RgbaImage};
use ksni::{menu::*, Icon, MenuItem, Tray, TrayMethods};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};
use whatszan_core::translations::t as translate;

use crate::{
    app_state::AppState,
    assets,
    webview::{open_settings, set_dnd_mute, toggle_main_window},
};

pub struct WhatsZanTray {
    pub app: AppHandle,
    pub app_name: String,
    pub dnd: bool,
    pub unread: u32,
    pub translations: Map<String, Value>,
    pub normal_icon: Option<PathBuf>,
    pub unread_icon: Option<PathBuf>,
}

impl WhatsZanTray {
    pub fn new(app: AppHandle) -> Self {
        let (app_name, dnd, translations, normal_icon, unread_icon) = {
            let state = app.state::<AppState>();
            let persist = state.persist.read().unwrap();
            let app_name = persist
                .get_str("custom-app-name", "WhatsZan")
                .trim()
                .to_string();
            (
                if app_name.is_empty() {
                    "WhatsZan".to_string()
                } else {
                    app_name
                },
                persist.get_bool("dnd", false),
                state.translations(),
                persist
                    .get_str("custom_tray_normal", "")
                    .to_string()
                    .into_option(),
                persist
                    .get_str("custom_tray_unread", "")
                    .to_string()
                    .into_option(),
            )
        };

        Self {
            app,
            app_name,
            dnd,
            unread: 0,
            translations,
            normal_icon,
            unread_icon,
        }
    }

    fn set_dnd(&mut self, enabled: bool) {
        self.dnd = enabled;

        let state = self.app.state::<AppState>();
        state.set_dnd(enabled);
        {
            let mut persist = state.persist.write().unwrap();
            let _ = persist.set("dnd", json!(enabled));
        }
        if let Some(window) = state.main_window() {
            set_dnd_mute(&window, enabled);
        }
        tracing::info!(enabled, "[DND] state changed");
    }

    fn icon_for(&self) -> Option<Icon> {
        if self.unread > 0 {
            if let Some(path) = self.unread_icon.as_deref().filter(|path| path.is_file()) {
                return load_icon(path, 64);
            }
        }

        let custom = self.normal_icon.as_deref().filter(|path| path.is_file());
        match (self.unread > 0, custom) {
            (true, Some(path)) => load_badged_icon(path, self.unread, 64),
            (true, None) => load_badged_icon_bytes(assets::APP_ICON_PNG, self.unread, 64),
            (false, Some(path)) => load_icon(path, 64),
            (false, None) => load_icon_bytes(assets::APP_ICON_PNG, 64),
        }
    }
}

impl Tray for WhatsZanTray {
    fn id(&self) -> String {
        "org.uznbt.whatszan".into()
    }

    fn title(&self) -> String {
        self.app_name.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        // Shells such as Quickshell render this as `title • description`.
        // Leaving the description empty avoids showing the app name twice.
        ksni::ToolTip {
            title: self.app_name.clone(),
            description: String::new(),
            ..Default::default()
        }
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Communications
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon_for().into_iter().collect()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = toggle_main_window(&self.app);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: translate(&self.translations, "show_hide", "Show/Hide").into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = toggle_main_window(&tray.app);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: translate(&self.translations, "settings", "Pengaturan...").into(),
                activate: Box::new(|tray: &mut Self| {
                    open_settings(&tray.app);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: translate(&self.translations, "dnd_mode", "Mode Jangan Ganggu (DND)").into(),
                checked: self.dnd,
                activate: Box::new(|tray: &mut Self| {
                    let next = !tray.dnd;
                    tray.set_dnd(next);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: translate(&self.translations, "quit", "Quit").into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    if let Some(state) = tray.app.try_state::<AppState>() {
                        state.set_quitting(true);
                    }
                    tray.app.exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn spawn(app: AppHandle) {
    let tray = WhatsZanTray::new(app.clone());
    tauri::async_runtime::spawn(async move {
        match tray.spawn().await {
            Ok(handle) => {
                if let Some(state) = app.try_state::<AppState>() {
                    *state.tray.lock().unwrap() = Some(handle);
                }
                tracing::info!("StatusNotifierItem tray registered");
            }
            Err(err) => tracing::warn!(%err, "cannot register tray icon"),
        }
    });
}

pub async fn set_unread(app: &AppHandle, unread: u32) {
    let handle = {
        let state = app.state::<AppState>();
        let tray = state.tray.lock().unwrap();
        tray.clone()
    };

    if let Some(handle) = handle {
        let _ = handle
            .update(|tray: &mut WhatsZanTray| {
                tray.unread = unread;
            })
            .await;
    }
}

/// Re-read branding (name, icons, translations) after the settings window
/// changes it, so the tray updates without restarting the app.
pub async fn refresh(app: &AppHandle) {
    let (normal, unread, translations, app_name) = {
        let state = app.state::<AppState>();
        let persist = state.persist.read().unwrap();
        let app_name = persist
            .get_str("custom-app-name", "WhatsZan")
            .trim()
            .to_string();
        (
            persist
                .get_str("custom_tray_normal", "")
                .to_string()
                .into_option(),
            persist
                .get_str("custom_tray_unread", "")
                .to_string()
                .into_option(),
            state.translations(),
            if app_name.is_empty() {
                "WhatsZan".to_string()
            } else {
                app_name
            },
        )
    };

    let handle = {
        let state = app.state::<AppState>();
        let tray = state.tray.lock().unwrap();
        tray.clone()
    };

    if let Some(handle) = handle {
        let _ = handle
            .update(|tray: &mut WhatsZanTray| {
                tray.normal_icon = normal;
                tray.unread_icon = unread;
                tray.translations = translations;
                tray.app_name = app_name;
            })
            .await;
    }
}

trait IntoOption {
    fn into_option(self) -> Option<PathBuf>;
}

impl IntoOption for String {
    fn into_option(self) -> Option<PathBuf> {
        if self.is_empty() {
            None
        } else {
            Some(PathBuf::from(self))
        }
    }
}

fn load_icon(path: &Path, size: u32) -> Option<Icon> {
    let image = image::open(path).ok()?.to_rgba8();
    Some(resize_icon(image, size))
}

fn load_icon_bytes(bytes: &[u8], size: u32) -> Option<Icon> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(resize_icon(image, size))
}

fn load_badged_icon(path: &Path, unread: u32, size: u32) -> Option<Icon> {
    let image = image::open(path).ok()?.to_rgba8();
    Some(resize_badged_icon(image, unread, size))
}

fn load_badged_icon_bytes(bytes: &[u8], unread: u32, size: u32) -> Option<Icon> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(resize_badged_icon(image, unread, size))
}

fn resize_icon(image: RgbaImage, size: u32) -> Icon {
    to_icon(image::imageops::resize(
        &image,
        size,
        size,
        FilterType::Lanczos3,
    ))
}

fn resize_badged_icon(image: RgbaImage, unread: u32, size: u32) -> Icon {
    let mut resized = image::imageops::resize(&image, size, size, FilterType::Lanczos3);
    if unread > 0 {
        draw_badge(&mut resized);
    }
    to_icon(resized)
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

fn to_icon(image: RgbaImage) -> Icon {
    let width = image.width() as i32;
    let height = image.height() as i32;
    let mut data = image.into_raw();
    // ksni expects ARGB32 in network byte order.
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
    Icon {
        width,
        height,
        data,
    }
}
