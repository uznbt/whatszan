use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex, RwLock,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::WebviewWindow;
use whatszan_core::{
    keys::{merged_keys, parsed_bindings},
    paths,
    translations::load_translations,
    JsonConfig, DEFAULT_USER_AGENT,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRequest {
    #[serde(rename = "type")]
    pub kind: String,
    pub files: Vec<String>,
    #[serde(rename = "toastText")]
    pub toast_text: String,
}

pub struct AppState {
    pub user_data: PathBuf,
    pub config: RwLock<JsonConfig>,
    pub persist: RwLock<JsonConfig>,
    /// Parse error captured before the config was moved into this struct.
    /// Electron shows a message box and lets the user continue with defaults
    /// or quit; the Rust build does the same after the main window exists.
    pub config_error: Option<String>,
    pub config_file: PathBuf,
    pub translations: RwLock<Map<String, Value>>,
    pub pending_share: Mutex<Option<ShareRequest>>,
    pub pending_jump: Mutex<Option<Value>>,
    pub dnd: AtomicBool,
    pub quitting: AtomicBool,
    pub settings_ignore_blur: AtomicBool,
    pub theme_dark: AtomicBool,
    pub main_page_loaded: AtomicBool,
    pub main_page_ready: AtomicBool,
    pub unread: AtomicU32,
    pub show_at_startup: bool,
    pub is_debug: bool,
    pub renderer_shortcuts: bool,
    pub tray: Mutex<Option<ksni::Handle<crate::tray::WhatsZanTray>>>,
    pub dbus: Mutex<Option<zbus::Connection>>,
    pub main_window: RwLock<Option<WebviewWindow>>,
}

impl AppState {
    pub fn new(
        user_data: PathBuf,
        config: JsonConfig,
        persist: JsonConfig,
        show_at_startup: bool,
        is_debug: bool,
    ) -> Self {
        let app_language = config.get_str("app-language", "auto").to_string();
        let locale = if app_language == "auto" {
            system_locale()
        } else {
            app_language
        };
        let translations = load_translations(&locale, &paths::locales_dir());
        let dnd = persist.get_bool("dnd", false);
        let config_error = config.parse_error().map(str::to_string);
        let config_file = config.file().to_path_buf();

        Self {
            user_data,
            config: RwLock::new(config),
            persist: RwLock::new(persist),
            config_error,
            config_file,
            translations: RwLock::new(translations),
            pending_share: Mutex::new(None),
            pending_jump: Mutex::new(None),
            dnd: AtomicBool::new(dnd),
            quitting: AtomicBool::new(false),
            settings_ignore_blur: AtomicBool::new(false),
            theme_dark: AtomicBool::new(false),
            main_page_loaded: AtomicBool::new(false),
            main_page_ready: AtomicBool::new(false),
            unread: AtomicU32::new(0),
            show_at_startup,
            is_debug,
            renderer_shortcuts: is_wayland_session(),
            tray: Mutex::new(None),
            dbus: Mutex::new(None),
            main_window: RwLock::new(None),
        }
    }

    pub fn custom_app_name(&self) -> String {
        let persist = self.persist.read().unwrap();
        persist
            .get_str("custom-app-name", "WhatsZan")
            .trim()
            .to_string()
            .if_empty("WhatsZan")
    }

    pub fn user_agent(&self) -> String {
        let config = self.config.read().unwrap();
        config.get_str("user-agent", DEFAULT_USER_AGENT).to_string()
    }

    pub fn app_language(&self) -> String {
        let config = self.config.read().unwrap();
        let configured = config.get_str("app-language", "auto");
        if configured == "auto" {
            system_locale()
        } else {
            configured.to_string()
        }
    }

    /// Languages WebKit should advertise in `Accept-Language` and use for
    /// text rendering. Electron appends `,en;q=0.9` when `app-language` is not
    /// `auto`; otherwise it uses the system preference list.
    pub fn preferred_languages(&self) -> Vec<String> {
        let configured = {
            let config = self.config.read().unwrap();
            config.get_str("app-language", "auto").to_string()
        };

        if configured == "auto" {
            return system_preferred_languages();
        }

        let mut languages = vec![configured];
        if !languages.iter().any(|language| language == "en") {
            languages.push("en".into());
        }
        languages
    }

    /// Languages for WebKit's spellchecker. The Electron build defaults to the
    /// system languages when `spellcheck-languages` is not configured.
    pub fn spellcheck_languages(&self) -> Vec<String> {
        let configured: Vec<String> = {
            let config = self.config.read().unwrap();
            config
                .get_array("spellcheck-languages")
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .filter(|value| !value.trim().is_empty())
                .collect()
        };

        if configured.is_empty() {
            system_preferred_languages()
        } else {
            configured
        }
    }

    pub fn reload_translations(&self) -> Map<String, Value> {
        let locale = self.app_language();
        let translations = load_translations(&locale, &paths::locales_dir());
        *self.translations.write().unwrap() = translations.clone();
        translations
    }

    pub fn translations(&self) -> Map<String, Value> {
        self.translations.read().unwrap().clone()
    }

    pub fn main_window(&self) -> Option<WebviewWindow> {
        self.main_window.read().unwrap().clone()
    }

    pub fn set_main_window(&self, window: WebviewWindow) {
        *self.main_window.write().unwrap() = Some(window);
    }

    pub fn is_dnd(&self) -> bool {
        self.dnd.load(Ordering::Relaxed)
    }

    pub fn set_dnd(&self, enabled: bool) {
        self.dnd.store(enabled, Ordering::Relaxed);
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::Relaxed)
    }

    pub fn set_quitting(&self, quitting: bool) {
        self.quitting.store(quitting, Ordering::Relaxed);
    }

    /// Values exposed to `window.ipc.stateGet(name)` in the renderer.
    pub fn state_value(&self, name: &str) -> Value {
        match name {
            "windowBounds" => {
                let persist = self.persist.read().unwrap();
                persist.get_or("window-bounds", json!({ "width": 1099, "height": 800 }))
            }
            "privacy-blur" => {
                let persist = self.persist.read().unwrap();
                json!(persist.get_bool("privacy-blur", false))
            }
            "anti-screencast" => {
                let persist = self.persist.read().unwrap();
                json!(persist.get_bool("anti-screencast", false))
            }
            "custom-app-name" => {
                let persist = self.persist.read().unwrap();
                let name = persist.get_str("custom-app-name", "WhatsZan").trim();
                json!(if name.is_empty() { "WhatsZan" } else { name })
            }
            "showAtStartup" => json!(self.show_at_startup),
            "rendererShortcuts" => json!(self.renderer_shortcuts),
            "keys" => {
                let config = self.config.read().unwrap();
                Value::Object(merged_keys(&config.get_object("keys")))
            }
            _ => {
                let config = self.config.read().unwrap();
                match name {
                    "notifPrefix" => json!(config.get_str("notification-prefix", "")),
                    "escToggle" => json!(config.get_bool("esc-toggle-window", false)),
                    "textReplacements" => config.get_or("text-replacements", json!({})),
                    "userAgent" => json!(config.get_str("user-agent", DEFAULT_USER_AGENT)),
                    "iconsDir" => {
                        let default_dir = self.user_data.join("user-icons");
                        json!(config.get_str("icons-directory", &default_dir.display().to_string()))
                    }
                    "incognito-noread" => json!(config.get_bool("incognito-noread", false)),
                    "incognito-notyping" => json!(config.get_bool("incognito-notyping", false)),
                    "incognito-antidelete" => json!(config.get_bool("incognito-antidelete", false)),
                    "incognito-statussaver" => {
                        json!(config.get_bool("incognito-statussaver", false))
                    }
                    _ => Value::Null,
                }
            }
        }
    }

    /// Snapshot of every value the renderer shim needs.
    ///
    /// The DOM integration is JavaScript by necessity, but it no longer makes
    /// IPC round-trips for feature decisions: Rust composes this object and
    /// injects it as `window.__wzConfig` before the shim runs.
    pub fn renderer_config(&self) -> Value {
        let config = self.config.read().unwrap();
        let persist = self.persist.read().unwrap();
        let app_name = persist.get_str("custom-app-name", "WhatsZan").trim();
        let app_name = if app_name.is_empty() {
            "WhatsZan"
        } else {
            app_name
        };
        json!({
            "appName": app_name,
            "debug": self.is_debug,
            "notifPrefix": config.get_str("notification-prefix", ""),
            "keys": Value::Object(merged_keys(&config.get_object("keys"))),
            "keyBindings": parsed_bindings(&config.get_object("keys")),
            "textReplacements": config.get_or("text-replacements", json!({})),
            "incognito-noread": config.get_bool("incognito-noread", false),
            "incognito-notyping": config.get_bool("incognito-notyping", false),
            "incognito-antidelete": config.get_bool("incognito-antidelete", false),
            "incognito-statussaver": config.get_bool("incognito-statussaver", false),
            "rendererShortcuts": self.renderer_shortcuts,
            // Windows jump lists have no Linux equivalent; skipping the
            // feature avoids a 24-hour localStorage scan and IPC calls.
            "jumpList": false,
        })
    }

    /// Object consumed by the native GTK settings window.
    pub fn settings_value(&self) -> Value {
        let mut settings = {
            let config = self.config.read().unwrap();
            config.data().clone()
        };

        {
            let persist = self.persist.read().unwrap();
            settings.insert(
                "privacy-blur".into(),
                json!(persist.get_bool("privacy-blur", false)),
            );
            settings.insert(
                "anti-screencast".into(),
                json!(persist.get_bool("anti-screencast", false)),
            );
            settings.insert(
                "sidebar-width".into(),
                json!(persist.get_u64("sidebar-width", 220)),
            );
            settings.insert(
                "auto-run".into(),
                json!(persist.get_bool("auto-run", false)),
            );
            settings.insert(
                "desktop-shortcut".into(),
                json!(persist.get_bool("desktop-shortcut", true)),
            );
            settings.insert(
                "custom-app-name".into(),
                json!(persist.get_str("custom-app-name", "WhatsZan")),
            );
            settings.insert(
                "icon-choice-app".into(),
                json!(persist.get_str("icon-choice-app", "whatszan")),
            );
            settings.insert(
                "icon-choice-unread".into(),
                json!(persist.get_str("icon-choice-unread", "whatszan")),
            );
            settings.insert(
                "icon-choice-normal".into(),
                json!(persist.get_str("icon-choice-normal", "whatszan")),
            );
        }

        Value::Object(settings)
    }

    pub fn is_theme_dark(&self) -> bool {
        self.theme_dark.load(Ordering::Relaxed)
    }

    pub fn set_theme_dark(&self, dark: bool) -> bool {
        self.theme_dark.swap(dark, Ordering::Relaxed) != dark
    }

    pub fn set_unread(&self, unread: u32) -> u32 {
        self.unread.swap(unread, Ordering::Relaxed)
    }

    pub fn settings_ignore_blur(&self) -> bool {
        self.settings_ignore_blur.load(Ordering::Relaxed)
    }

    pub fn set_settings_ignore_blur(&self, ignore: bool) {
        self.settings_ignore_blur.store(ignore, Ordering::Relaxed);
    }

    pub fn is_main_page_loaded(&self) -> bool {
        self.main_page_loaded.load(Ordering::Relaxed)
    }

    pub fn set_main_page_loaded(&self, loaded: bool) {
        self.main_page_loaded.store(loaded, Ordering::Relaxed);
    }

    /// Whether the integration script has already been injected into the
    /// current document. `page_ready` is emitted by the bridge on the real
    /// page; WebKit can still expose the previous `about:blank` context when
    /// Tauri's `PageLoadEvent::Finished` fires, so this flag prevents the
    /// delayed fallback from injecting twice.
    pub fn is_main_page_ready(&self) -> bool {
        self.main_page_ready.load(Ordering::Relaxed)
    }

    pub fn set_main_page_ready(&self, ready: bool) {
        self.main_page_ready.store(ready, Ordering::Relaxed);
    }
}

trait EmptyString {
    fn if_empty(self, fallback: &str) -> String;
}

impl EmptyString for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

/// Chromium's globalShortcut uses XGrabKey and therefore only works on X11.
/// On Wayland the privacy shortcuts are handled by the renderer instead.
pub fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Best-effort equivalent of Electron's `app.getPreferredSystemLanguages()`.
///
/// `LANGUAGE` is a colon-separated priority list on glibc systems. When it is
/// unset, the usual `LC_ALL` / `LC_MESSAGES` / `LANG` fallback is used. Tags
/// are normalized from `id_ID.UTF-8` to `id-ID`, and the bare language tag is
/// appended as a lower-priority fallback.
pub fn system_preferred_languages() -> Vec<String> {
    let mut raw: Vec<String> = Vec::new();
    if let Ok(language) = std::env::var("LANGUAGE") {
        raw.extend(
            language
                .split(':')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        );
    }
    if raw.is_empty() {
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(key) {
                if !value.is_empty() {
                    raw.push(value);
                    break;
                }
            }
        }
    }

    normalize_preferred_languages(&raw)
}

/// Normalize locale tags such as `id_ID.UTF-8` into the `id-ID` + `id` pair
/// WebKit expects, dropping duplicates and `C`/`POSIX` placeholders.
fn normalize_preferred_languages(raw: &[String]) -> Vec<String> {
    let mut languages: Vec<String> = Vec::new();
    for entry in raw {
        let without_encoding = entry.split('.').next().unwrap_or(entry);
        let without_modifier = without_encoding
            .split('@')
            .next()
            .unwrap_or(without_encoding);
        let normalized = without_modifier.replace('_', "-");
        let language = normalized.split('-').next().unwrap_or("").to_string();
        if language.is_empty() || language == "C" || language == "POSIX" {
            continue;
        }

        if !languages
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&normalized))
        {
            languages.push(normalized);
        }
        if !languages
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&language))
        {
            languages.push(language);
        }
    }

    if languages.is_empty() {
        languages.push("en-US".into());
        languages.push("en".into());
    }
    languages
}

#[cfg(test)]
mod tests {
    use super::normalize_preferred_languages;

    fn normalize(values: &[&str]) -> Vec<String> {
        normalize_preferred_languages(
            &values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn normalizes_encoding_and_territory() {
        assert_eq!(normalize(&["id_ID.UTF-8"]), ["id-ID", "id"]);
        assert_eq!(normalize(&["en_US.UTF-8@euro"]), ["en-US", "en"]);
    }

    #[test]
    fn drops_placeholders_and_duplicates() {
        assert_eq!(normalize(&["C", "POSIX"]), ["en-US", "en"]);
        assert_eq!(normalize(&["id_ID.UTF-8", "id-ID", "id"]), ["id-ID", "id"]);
    }

    #[test]
    fn preserves_priority_order() {
        assert_eq!(
            normalize(&["de_DE.UTF-8", "fr_FR.UTF-8", "en_US.UTF-8"]),
            ["de-DE", "de", "fr-FR", "fr", "en-US", "en"]
        );
    }
}

/// Best-effort equivalent of Electron's `app.getLocale()`.
pub fn system_locale() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if value.is_empty() {
                continue;
            }
            let without_encoding = value.split('.').next().unwrap_or(&value);
            let language = without_encoding
                .split('_')
                .next()
                .unwrap_or(without_encoding);
            if !language.is_empty() && language != "C" && language != "POSIX" {
                return language.to_string();
            }
        }
    }
    "en".to_string()
}
