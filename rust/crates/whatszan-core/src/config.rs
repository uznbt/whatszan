//! Encrypted JSON configuration, byte-compatible with `src/json-config.mjs`.
//!
//! Node derives the key with `crypto.scryptSync(password, "salt", 32)`
//! (N=16384, r=8, p=1) and encrypts with AES-256-GCM using a random 16-byte
//! IV. The payload is stored as `iv_hex:tag_hex:ciphertext_hex`. RustCrypto's
//! AES-GCM accepts the same 16-byte nonce, so existing WhatsZan profiles can
//! be read without a migration step.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use aes_gcm::{
    aead::{consts::U16, Aead, KeyInit},
    aes::Aes256,
    AesGcm, Nonce,
};
use rand::RngCore;
use scrypt::{scrypt, Params};
use serde_json::{Map, Value};

const PASSWORD: &str = "whatszan-super-secret-key-2026";
const SALT: &[u8] = b"salt";
const IV_LEN: usize = 16;
const TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;

type Aes256Gcm16 = AesGcm<Aes256, U16>;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
}

fn encryption_key() -> &'static [u8; KEY_LEN] {
    static KEY: OnceLock<[u8; KEY_LEN]> = OnceLock::new();
    KEY.get_or_init(|| {
        // Node's scrypt defaults: N=2^14, r=8, p=1.
        let params = Params::new(14, 8, 1, KEY_LEN).expect("valid scrypt parameters");
        let mut key = [0u8; KEY_LEN];
        scrypt(PASSWORD.as_bytes(), SALT, &params, &mut key).expect("scrypt key derivation");
        key
    })
}

/// Encrypt a UTF-8 string using the exact format written by the Electron app.
pub fn encrypt(plaintext: &str) -> Result<String, ConfigError> {
    let cipher = Aes256Gcm16::new_from_slice(encryption_key())
        .map_err(|err| ConfigError::Crypto(err.to_string()))?;

    let mut iv = [0u8; IV_LEN];
    rand::thread_rng().fill_bytes(&mut iv);
    let nonce = Nonce::<U16>::from_slice(&iv);

    let mut sealed = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|err| ConfigError::Crypto(err.to_string()))?;

    // RustCrypto appends the 16-byte authentication tag; Node stores it
    // separately in the second colon-separated field.
    let tag = sealed.split_off(sealed.len() - TAG_LEN);
    Ok(format!(
        "{}:{}:{}",
        hex::encode(iv),
        hex::encode(tag),
        hex::encode(sealed)
    ))
}

/// Decrypt a payload written by either this crate or `src/json-config.mjs`.
/// Returns `None` for anything that is not a valid encrypted payload, which
/// lets callers fall back to plain JSON exactly like the Node implementation.
pub fn decrypt(payload: &str) -> Option<String> {
    let mut parts = payload.split(':');
    let iv_hex = parts.next()?;
    let tag_hex = parts.next()?;
    let cipher_hex = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let iv = hex::decode(iv_hex).ok()?;
    let tag = hex::decode(tag_hex).ok()?;
    let ciphertext = hex::decode(cipher_hex).ok()?;
    if iv.len() != IV_LEN || tag.len() != TAG_LEN {
        return None;
    }

    let cipher = Aes256Gcm16::new_from_slice(encryption_key()).ok()?;
    let nonce = Nonce::<U16>::from_slice(&iv);

    let mut sealed = ciphertext;
    sealed.extend_from_slice(&tag);

    let plaintext = cipher.decrypt(nonce, sealed.as_ref()).ok()?;
    String::from_utf8(plaintext).ok()
}

/// Drop-in replacement for the `JsonConfig` class in `src/json-config.mjs`.
#[derive(Debug)]
pub struct JsonConfig {
    file: PathBuf,
    data: Map<String, Value>,
    parse_error: Option<String>,
}

impl JsonConfig {
    /// Load `file`, using `defaults` when the file does not exist or cannot be
    /// parsed. The Electron app passes an empty map for both its config and
    /// persistent-state files; callers apply defaults through the getters.
    pub fn new(file: impl Into<PathBuf>, defaults: Map<String, Value>) -> Self {
        let file = file.into();
        let mut config = Self {
            file,
            data: Map::new(),
            parse_error: None,
        };
        config.reload(defaults);
        config
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn parse_error(&self) -> Option<&str> {
        self.parse_error.as_deref()
    }

    pub fn data(&self) -> &Map<String, Value> {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut Map<String, Value> {
        &mut self.data
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn get_or(&self, key: &str, default: Value) -> Value {
        self.data.get(key).cloned().unwrap_or(default)
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.data
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn get_str<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.data
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
    }

    pub fn get_u64(&self, key: &str, default: u64) -> u64 {
        self.data
            .get(key)
            .and_then(Value::as_u64)
            .unwrap_or(default)
    }

    pub fn get_f64(&self, key: &str, default: f64) -> f64 {
        self.data
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }

    pub fn get_array(&self, key: &str) -> Vec<Value> {
        self.data
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_object(&self, key: &str) -> Map<String, Value> {
        self.data
            .get(key)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) -> Result<(), ConfigError> {
        self.data.insert(key.into(), value);
        self.save()
    }

    pub fn delete(&mut self, key: &str) -> Result<(), ConfigError> {
        self.data.remove(key);
        self.save()
    }

    /// Apply several changes with a single encrypted write.
    pub fn update<F>(&mut self, f: F) -> Result<(), ConfigError>
    where
        F: FnOnce(&mut Map<String, Value>),
    {
        f(&mut self.data);
        self.save()
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)?;
        }

        let raw = serde_json::to_string(&self.data)?;
        let encrypted = encrypt(&raw)?;

        // Write through a sibling file so a crash cannot leave a truncated
        // config behind. Electron writes in place; this is strictly safer.
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, encrypted)?;
        fs::rename(&tmp, &self.file)?;
        Ok(())
    }

    fn reload(&mut self, defaults: Map<String, Value>) {
        if !self.file.exists() {
            self.data = defaults;
            return;
        }

        let raw = match fs::read_to_string(&self.file) {
            Ok(raw) => raw,
            Err(err) => {
                self.parse_error = Some(err.to_string());
                self.data = defaults;
                return;
            }
        };

        // Encrypted payload first, then raw JSON for profiles written by
        // older WhatsZan versions (the Node class does the same fallback).
        let parsed = decrypt(raw.trim())
            .and_then(|plain| serde_json::from_str::<Value>(&plain).ok())
            .or_else(|| serde_json::from_str::<Value>(&raw).ok());

        match parsed {
            Some(Value::Object(map)) => self.data = map,
            Some(other) => {
                self.parse_error = Some(format!("expected a JSON object, got {other}"));
                self.data = defaults;
            }
            None => {
                self.parse_error = Some("invalid JSON or encrypted payload".to_string());
                self.data = defaults;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_encryption() {
        let payload = r#"{"hello":"dunia","n":42}"#;
        let sealed = encrypt(payload).expect("encrypt");
        assert_eq!(sealed.split(':').count(), 3);
        assert_eq!(decrypt(&sealed).as_deref(), Some(payload));
    }

    #[test]
    fn decrypts_node_payload() {
        // Generated with the Electron implementation:
        // node -e '...crypto.scryptSync...'
        let payload = include_str!("../fixtures/node-encrypted.txt").trim();
        assert_eq!(
            decrypt(payload).as_deref(),
            Some(r#"{"log-level":"info","quit-on-close":false}"#)
        );
    }

    #[test]
    fn config_roundtrip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        let mut config = JsonConfig::new(&path, Map::new());
        config.set("log-level", json!("debug")).unwrap();
        config.set("quit-on-close", json!(true)).unwrap();

        let reloaded = JsonConfig::new(&path, Map::new());
        assert_eq!(reloaded.get_str("log-level", "info"), "debug");
        assert!(reloaded.get_bool("quit-on-close", false));
        assert!(reloaded.parse_error().is_none());

        // The file must not contain plaintext JSON.
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("log-level"));
    }

    #[test]
    fn plain_json_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        fs::write(&path, r#"{"legacy":true}"#).unwrap();

        let config = JsonConfig::new(&path, Map::new());
        assert!(config.get_bool("legacy", false));
    }
}
