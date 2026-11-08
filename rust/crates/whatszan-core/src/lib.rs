//! Platform-independent core for the WhatsZan Rust rewrite.
//!
//! The Electron implementation lives in the repository root and is kept
//! untouched. This crate ports the behaviour that does not depend on a
//! concrete GUI toolkit: the encrypted JSON config, session snapshots,
//! WhatsApp URL routing, keyboard defaults, translations, and the Linux
//! desktop/autostart integration.

pub mod config;
pub mod keys;
pub mod mime;
pub mod paths;
pub mod session_backup;
pub mod shortcuts;
pub mod translations;
pub mod url;

pub use config::{ConfigError, JsonConfig};
pub use mime::mime_for_name;
pub use session_backup::{
    backup_session_data, latest_snapshot_name, request_session_restore,
    restore_session_backup_if_requested,
};
pub use translations::{load_translations, translation};
pub use url::{
    convert_whatsapp_url, extract_real_url, is_web_whatsapp, is_whatsapp_internal,
    unread_count_from_favicon,
};

/// The user agent the Electron client uses. WhatsApp Web is served the same
/// desktop bundle for every Chromium-based browser, so keeping this value
/// avoids a "browser not supported" screen in WebKitGTK as well.
///
/// Keep the `Electron/<version>` token: WhatsApp parses the user agent with
/// ua-parser-js, whose Electron rule runs before its Chrome rule. The resulting
/// browser name is mapped to `DeviceProps$PlatformType.DESKTOP`, which is what
/// the Electron build reports. A plain Chrome UA would instead be shown as
/// "Chrome" (with a browser logo) on the linked-devices screen.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) ",
    "WhatsZan/",
    env!("CARGO_PKG_VERSION"),
    " Chrome/144.0.7559.236 Electron/40.10.5 Safari/537.36"
);
