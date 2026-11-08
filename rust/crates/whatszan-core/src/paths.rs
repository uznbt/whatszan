//! Filesystem locations shared by the Rust binary.
//!
//! Electron's `app.getPath("userData")` resolves to `~/.config/WhatsZan` on
//! Linux because `productName` is `WhatsZan`. Keeping that directory means the
//! Rust client can reuse the existing config and WhatsApp session instead of
//! asking the user to scan a QR code again.

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "WhatsZan";

/// Override with `WHATZAN_USER_DATA` for tests or portable installs.
pub fn user_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("WHATZAN_USER_DATA") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join(APP_DIR_NAME)
}

pub fn config_file() -> PathBuf {
    user_data_dir().join("config.json")
}

pub fn persistent_state_file() -> PathBuf {
    user_data_dir().join("persistent-state.json")
}

pub fn session_backup_dir() -> PathBuf {
    user_data_dir().join("session-backup")
}

pub fn user_scripts_dir() -> PathBuf {
    user_data_dir().join("user-scripts")
}

pub fn user_icons_dir() -> PathBuf {
    user_data_dir().join("user-icons")
}

/// Directory that contains `en.json`, `id.json`, ... In a source checkout this
/// is `<rust>/locales`; installed builds use `/usr/share/whatszan/locales`.
pub fn locales_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("WHATZAN_LOCALES_DIR") {
        return PathBuf::from(path);
    }

    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
    if dev.is_dir() {
        return dev;
    }

    PathBuf::from("/usr/share/whatszan/locales")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_wins() {
        std::env::set_var("WHATZAN_USER_DATA", "/tmp/whatszan-test-profile");
        assert_eq!(user_data_dir(), PathBuf::from("/tmp/whatszan-test-profile"));
        std::env::remove_var("WHATZAN_USER_DATA");
    }
}
