//! Default artwork compiled into the binary.
//!
//! Every image lives under `rust/crates/whatszan-app/assets`, so the Rust tree
//! no longer borrows files from the Electron sources at build time. Branding
//! still works when `/usr/share/whatszan` is absent, and user-selected custom
//! icons remain ordinary files on disk.

/// Default application/tray icon (512×512 PNG).
pub const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app.png");

/// Default "WhatsApp" branding choice used by the settings window.
pub const WHATSAPP_ICON_PNG: &[u8] = include_bytes!("../assets/whatsapp.png");

/// Badged artwork used as the preview for the unread tray icon choice.
pub const APP_BADGED_ICON_PNG: &[u8] = include_bytes!("../assets/app-badged.png");

/// Window/taskbar icon; same artwork as the Electron `build/icon.png`.
pub const WINDOW_ICON_PNG: &[u8] = include_bytes!("../assets/app.png");

#[cfg(test)]
mod tests {
    use super::*;

    fn is_png(bytes: &[u8]) -> bool {
        bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
    }

    #[test]
    fn embeds_png_icons() {
        assert!(is_png(APP_ICON_PNG));
        assert!(is_png(APP_BADGED_ICON_PNG));
        assert!(is_png(WHATSAPP_ICON_PNG));
        assert!(is_png(WINDOW_ICON_PNG));
    }
}
