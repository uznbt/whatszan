//! Filesystem locations shared by the Rust binary.
//!
//! Electron's `app.getPath("userData")` resolves to `~/.config/WhatsZan` on
//! Linux and `%APPDATA%\WhatsZan` on Windows because `productName` is
//! `WhatsZan`. Keeping that directory means the Rust client can reuse the
//! existing config and WhatsApp session instead of asking the user to scan a
//! QR code again.

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "WhatsZan";

/// Override with `WHATZAN_USER_DATA` for tests or portable installs.
pub fn user_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("WHATZAN_USER_DATA") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = portable_user_data_dir() {
        return path;
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join(APP_DIR_NAME)
}

/// Keep portable Windows installs working: a `portable` file next to the
/// executable redirects the profile to `WhatsZan-data` in the same folder.
#[cfg(target_os = "windows")]
fn portable_user_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    if dir.join("portable").is_file() {
        Some(dir.join("WhatsZan-data"))
    } else {
        None
    }
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
/// is `<repo>/locales`; installed builds use `/usr/share/whatszan/locales`.
pub fn locales_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("WHATZAN_LOCALES_DIR") {
        return PathBuf::from(path);
    }

    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
    if dev.is_dir() {
        return dev;
    }

    // Windows installers keep the catalogs next to the executable. They are
    // also embedded into the binary, so a missing folder only disables
    // drop-in overrides.
    #[cfg(target_os = "windows")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let installed = dir.join("locales");
            if installed.is_dir() {
                return installed;
            }
        }
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
