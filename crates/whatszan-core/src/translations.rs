//! Locale loading, ported from `loadTranslations` in `src/util.mjs`.
//!
//! Catalogs are compiled into `whatszan-core` from `locales/`, so the Rust
//! tree is self-contained and an installed build does not need any files from
//! the Electron sources. A `WHATZAN_LOCALES_DIR` override still takes
//! precedence, which keeps translation work and testing convenient.

use std::{fs, io::ErrorKind, path::Path};

use serde_json::{Map, Value};

/// Mirrors the JSON files in `locales/`.
const EMBEDDED_LOCALES: &[(&str, &str)] = &[
    ("de", include_str!("../../../locales/de.json")),
    ("en", include_str!("../../../locales/en.json")),
    ("fr", include_str!("../../../locales/fr.json")),
    ("id", include_str!("../../../locales/id.json")),
    ("it", include_str!("../../../locales/it.json")),
    ("pt", include_str!("../../../locales/pt.json")),
];

fn parse_catalog(raw: &str, source: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => Some(map),
        Ok(_) => {
            tracing::warn!(source, "translation file is not a JSON object");
            None
        }
        Err(err) => {
            tracing::warn!(source, %err, "cannot parse translations");
            None
        }
    }
}

/// Look up a catalog compiled into the binary.
pub fn embedded_translations(language: &str) -> Option<Map<String, Value>> {
    EMBEDDED_LOCALES
        .iter()
        .find(|(candidate, _)| *candidate == language)
        .and_then(|(_, raw)| parse_catalog(raw, "embedded locale"))
}

/// Load `locales/<language>.json`. `locale` may be `id-ID`, `en-US`, etc.;
/// only the part before the first dash is used, exactly like the JS version.
/// Missing files fall back to the embedded catalog. Missing or malformed
/// catalogs yield an empty map.
pub fn load_translations(locale: &str, locales_dir: &Path) -> Map<String, Value> {
    let language = locale.split('-').next().unwrap_or(locale);
    let path = locales_dir.join(format!("{language}.json"));

    match fs::read_to_string(&path) {
        Ok(raw) => {
            if let Some(map) = parse_catalog(&raw, &path.display().to_string()) {
                return map;
            }
        }
        Err(err) if err.kind() != ErrorKind::NotFound => {
            tracing::warn!(path = %path.display(), %err, "cannot load translations");
        }
        Err(_) => {}
    }

    embedded_translations(language).unwrap_or_default()
}

pub fn translation<'a>(translations: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    translations.get(key).and_then(Value::as_str)
}

/// Return the translation or a static fallback.
pub fn t<'a>(translations: &'a Map<String, Value>, key: &str, fallback: &'a str) -> &'a str {
    translation(translations, key).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_indonesian_and_falls_back() {
        let locales = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
        let id = load_translations("id-ID", &locales);
        assert_eq!(
            translation(&id, "show_hide"),
            Some("Tampilkan / Sembunyikan")
        );

        let missing = load_translations("xx-YY", &locales);
        assert!(missing.is_empty());
    }

    #[test]
    fn falls_back_to_embedded_catalog() {
        let missing_dir = Path::new("/nonexistent/whatszan-locales");
        let id = load_translations("id-ID", missing_dir);
        assert_eq!(
            translation(&id, "show_hide"),
            Some("Tampilkan / Sembunyikan")
        );

        let en = load_translations("en-US", missing_dir);
        assert_eq!(translation(&en, "show_hide"), Some("Show/Hide"));
    }
}
