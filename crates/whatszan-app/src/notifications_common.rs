//! Helpers shared by the Linux and Windows notification backends.
//!
//! Both platforms need the same two pieces of artwork:
//!
//! * the application icon currently selected in the branding settings, and
//! * the sender avatar supplied by WhatsApp as a `data:` URL.
//!
//! Files are content-addressed so a branding change produces a new path. That
//! prevents notification servers and the Windows shell from showing a cached
//! copy of the previous icon.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::{app_state::AppState, assets};

/// Resolve the raw PNG bytes for the current application branding choice.
pub fn branding_bytes(state: &AppState) -> Vec<u8> {
    let (choice, custom) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_str("icon-choice-app", "whatszan").to_string(),
            persist.get_str("custom_tray_app", "").to_string(),
        )
    };

    match choice.as_str() {
        "custom" if !custom.is_empty() => match fs::read(&custom) {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!(path = %custom, %err, "custom notification icon is unreadable");
                assets::APP_ICON_PNG.to_vec()
            }
        },
        "whatsapp" => assets::WHATSAPP_ICON_PNG.to_vec(),
        _ => assets::APP_ICON_PNG.to_vec(),
    }
}

/// Resolve the application icon as a cached PNG file.
///
/// The branding choice is read at send time so a change applies to the next
/// notification without restarting the app.
pub fn branding_icon(state: &AppState) -> Option<PathBuf> {
    let bytes = branding_bytes(state);
    cache_icon(state, &bytes).or_else(|| cache_icon(state, assets::APP_ICON_PNG))
}

/// Decode the `data:image/...;base64,...` URL produced by the renderer and
/// store it as a notification image. Non-image payloads are rejected so a
/// broken avatar does not replace the branding icon with an error placeholder.
pub fn cache_avatar(state: &AppState, data_url: &str) -> Option<PathBuf> {
    let (header, encoded) = data_url.split_once(',')?;
    let mime = header.strip_prefix("data:")?.split(';').next()?;
    if !mime.starts_with("image/") {
        return None;
    }
    let bytes = BASE64.decode(encoded.trim()).ok()?;
    if image::load_from_memory(&bytes).is_err() {
        return None;
    }

    let extension = match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        _ => "img",
    };
    cache_bytes(
        state,
        &bytes,
        format!("notification-avatar-{:016x}.{extension}", hash(&bytes)),
    )
}

/// Store the branding icon as PNG under a content-addressed name.
///
/// Existing profiles may point `custom_tray_app` at an ICO or JPEG file
/// (Electron used `.ico` for the app icon). Windows toasts expect PNG artwork,
/// so the image is decoded and re-encoded instead of copying the raw bytes
/// under a `.png` extension.
fn cache_icon(state: &AppState, bytes: &[u8]) -> Option<PathBuf> {
    let path = state
        .user_data
        .join("cache")
        .join(format!("notification-{:016x}.png", hash(bytes)));

    if path.is_file() {
        return Some(path);
    }

    let image = image::load_from_memory(bytes).ok()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok()?;
    }
    image.save(&path).ok()?;
    Some(path)
}

pub(crate) fn cache_bytes(state: &AppState, bytes: &[u8], name: String) -> Option<PathBuf> {
    let path = state.user_data.join("cache").join(name);

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok()?;
    }
    if !path.is_file() {
        fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

pub(crate) fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn test_state(dir: &Path) -> AppState {
        let config = whatszan_core::JsonConfig::new(dir.join("config.json"), Default::default());
        let persist =
            whatszan_core::JsonConfig::new(dir.join("persistent-state.json"), Default::default());
        AppState::new(dir.to_path_buf(), config, persist, false, false)
    }

    fn decoded(path: &Path) -> image::RgbaImage {
        image::open(path).unwrap().to_rgba8()
    }

    fn decoded_asset(bytes: &[u8]) -> image::RgbaImage {
        image::load_from_memory(bytes).unwrap().to_rgba8()
    }

    #[test]
    fn notification_icon_follows_branding_choice() {
        let dir = std::env::temp_dir().join(format!("whatszan-notif-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let state = test_state(&dir);

        // The cached file is re-encoded PNG artwork, not a byte copy of the
        // embedded asset, so compare pixels instead of raw bytes.
        let app_icon = branding_icon(&state).unwrap();
        assert_eq!(decoded(&app_icon), decoded_asset(assets::APP_ICON_PNG));

        let custom_path = dir.join("custom.png");
        let custom = image::RgbaImage::from_fn(2, 2, |x, y| {
            image::Rgba([(x * 100) as u8, (y * 100) as u8, 42, 255])
        });
        custom.save(&custom_path).unwrap();
        {
            let mut persist = state.persist.write().unwrap();
            persist
                .set("icon-choice-app", serde_json::json!("custom"))
                .unwrap();
            persist
                .set(
                    "custom_tray_app",
                    serde_json::json!(custom_path.display().to_string()),
                )
                .unwrap();
        }
        let custom_icon = branding_icon(&state).unwrap();
        assert_ne!(custom_icon, app_icon);
        assert_eq!(decoded(&custom_icon), custom);

        // Existing profiles may store artwork Windows toasts cannot load
        // (or a corrupted file). Fall back to the embedded default.
        fs::write(&custom_path, b"custom-bytes").unwrap();
        let fallback_icon = branding_icon(&state).unwrap();
        assert_eq!(decoded(&fallback_icon), decoded_asset(assets::APP_ICON_PNG));

        {
            let mut persist = state.persist.write().unwrap();
            persist
                .set("icon-choice-app", serde_json::json!("whatsapp"))
                .unwrap();
        }
        let whatsapp_icon = branding_icon(&state).unwrap();
        assert_eq!(
            decoded(&whatsapp_icon),
            decoded_asset(assets::WHATSAPP_ICON_PNG)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn avatar_data_url_is_cached_and_validated() {
        let dir = std::env::temp_dir().join(format!("whatszan-avatar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let state = test_state(&dir);

        let data_url = format!(
            "data:image/png;base64,{}",
            BASE64.encode(assets::WHATSAPP_ICON_PNG)
        );
        let path = cache_avatar(&state, &data_url).unwrap();
        assert!(path.is_file());
        assert!(image::load_from_memory(&fs::read(&path).unwrap()).is_ok());

        assert!(cache_avatar(&state, "data:text/plain;base64,aGVsbG8=").is_none());
        assert!(cache_avatar(&state, "data:image/png;base64,bm90IGFuIGltYWdl").is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
