//! Default keyboard bindings, ported from `src/keys.mjs`.
//!
//! The renderer understands this exact shape:
//! `{ "whatsappAction": "..." }` or
//! `{ "action": "OPEN_NTH_CHAT", "chatIndex": 0 }` (plus IPC entries).

use serde::Serialize;
use serde_json::{json, Map, Value};

/// A key binding parsed from the `"<modifiers> <key>"` format used by the
/// Electron client. The renderer only has to compare these fields against a
/// DOM keyboard event.
#[derive(Debug, Clone, Serialize)]
pub struct KeyBinding {
    pub binding: String,
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub effect: Value,
}

pub fn default_keys() -> Map<String, Value> {
    let mut keys = Map::new();

    keys.insert(
        "A ArrowDown".into(),
        json!({ "whatsappAction": "GO_TO_NEXT_CHAT" }),
    );
    keys.insert(
        "A ArrowUp".into(),
        json!({ "whatsappAction": "GO_TO_PREV_CHAT" }),
    );
    keys.insert(
        "C Tab".into(),
        json!({ "whatsappAction": "GO_TO_NEXT_CHAT" }),
    );
    keys.insert(
        "CS Tab".into(),
        json!({ "whatsappAction": "GO_TO_PREV_CHAT" }),
    );
    keys.insert("C u".into(), json!({ "whatsappAction": "TOGGLE_UNREAD" }));
    keys.insert(
        "C ArrowUp".into(),
        json!({ "whatsappAction": "EDIT_LAST_MESSAGE" }),
    );
    keys.insert(
        "A l".into(),
        json!([
            { "action": "IPC", "method": "windowToggle" },
            { "whatsappAction": "LOCK_SCREEN" }
        ]),
    );

    for (index, key) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
        .iter()
        .enumerate()
    {
        keys.insert(
            format!("A {key}"),
            json!({ "action": "OPEN_NTH_CHAT", "chatIndex": index }),
        );
        keys.insert(
            format!("C {key}"),
            json!({ "action": "OPEN_NTH_CHAT", "chatIndex": index }),
        );
    }

    keys
}

/// Merge user overrides over the defaults, like `{ ...defaultKeys, ...config }`.
pub fn merged_keys(overrides: &Map<String, Value>) -> Map<String, Value> {
    let mut keys = default_keys();
    for (key, value) in overrides {
        keys.insert(key.clone(), value.clone());
    }
    keys
}

/// Parse a binding such as `"CS Tab"` or `"A 1"`.
///
/// Modifier letters follow the Electron convention: `C` control, `S` shift,
/// `A` alt, `M` meta/super. The key is everything after the first space so
/// names like `ArrowDown` stay intact.
pub fn parse_binding(binding: &str) -> KeyBinding {
    let (modifiers, key) = binding.split_once(' ').unwrap_or(("", binding));
    KeyBinding {
        binding: binding.to_string(),
        key: key.to_string(),
        ctrl: modifiers.contains('C'),
        shift: modifiers.contains('S'),
        alt: modifiers.contains('A'),
        meta: modifiers.contains('M'),
        effect: Value::Null,
    }
}

/// Parse every merged binding into the flat shape consumed by the renderer.
pub fn parsed_bindings(overrides: &Map<String, Value>) -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    for (binding, effect) in merged_keys(overrides) {
        let mut parsed = parse_binding(&binding);
        parsed.effect = effect;
        bindings.push(parsed);
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_same_number_of_defaults_as_the_electron_client() {
        // 7 explicit bindings + 10 number keys x 2 modifiers.
        assert_eq!(default_keys().len(), 27);
    }

    #[test]
    fn number_bindings_use_zero_based_indexes() {
        let keys = default_keys();
        assert_eq!(keys["A 1"]["chatIndex"], 0);
        assert_eq!(keys["C 0"]["chatIndex"], 9);
    }

    #[test]
    fn parses_modifiers_and_multi_character_keys() {
        let parsed = parse_binding("CS ArrowDown");
        assert_eq!(parsed.key, "ArrowDown");
        assert!(parsed.ctrl);
        assert!(parsed.shift);
        assert!(!parsed.alt);
        assert!(!parsed.meta);

        let parsed = parse_binding("A 1");
        assert_eq!(parsed.key, "1");
        assert!(!parsed.ctrl);
        assert!(!parsed.shift);
        assert!(parsed.alt);
        assert!(!parsed.meta);
    }

    #[test]
    fn parsed_bindings_keep_effects_and_override_defaults() {
        let mut overrides = Map::new();
        overrides.insert("C q".into(), json!({ "whatsappAction": "LOCK_SCREEN" }));

        let bindings = parsed_bindings(&overrides);
        assert_eq!(bindings.len(), default_keys().len() + 1);

        let custom = bindings
            .iter()
            .find(|binding| binding.binding == "C q")
            .expect("custom binding present");
        assert_eq!(custom.effect["whatsappAction"], "LOCK_SCREEN");
        assert!(custom.ctrl);
    }
}
