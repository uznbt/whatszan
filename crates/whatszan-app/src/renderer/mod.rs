//! Renderer assets embedded in the Rust binary.
//!
//! WebKitGTK does not expose its DOM to Rust. Everything that needs to touch
//! WhatsApp's DOM therefore remains a small JavaScript shim, but the shim is
//! compiled into the Rust binary and every feature decision, setting, and IPC
//! handler lives on the Rust side. The original Electron files are never read
//! at runtime.

pub mod bridge;
pub mod injected;
pub mod privacy;

/// Install or refresh `window.__wzConfig` and make the bridge's `stateGet`
/// resolve feature flags from that Rust-owned snapshot.
pub fn config_script(config: &serde_json::Value) -> String {
    let config = serde_json::to_string(config).unwrap_or_else(|_| "{}".into());
    format!(
        r#"(function() {{
  const config = {config};
  window.__wzConfig = config;
  const ipc = window.__wzIpc;
  if (!ipc || typeof ipc.stateGet !== 'function') return;
  if (typeof config.debug === 'boolean') ipc.debug = config.debug;
  if (!ipc.__wzOriginalStateGet) ipc.__wzOriginalStateGet = ipc.stateGet.bind(ipc);
  ipc.stateGet = (name) => Object.prototype.hasOwnProperty.call(config, name)
    ? Promise.resolve(config[name])
    : ipc.__wzOriginalStateGet(name);
}})();"#
    )
}

/// Build the integration script for the current document.
///
/// WRY reserves `window.ipc` on Linux for Tauri's own channel, so the shim is
/// rewritten to `window.__wzIpc` while it is composed here in Rust. Feature
/// setup is gated by the Rust config rather than being started unconditionally.
pub fn injected_script(app_name: &str, config: &serde_json::Value) -> String {
    let app_name = serde_json::to_string(app_name).unwrap_or_else(|_| "\"WhatsZan\"".into());
    let shim = injected::INJECTED_JS
        .replace("window?.ipc", "window.__wzIpc")
        .replace("window.ipc", "window.__wzIpc")
        .replace(
            "ewSetupAntiDelete();",
            "if (useAntiDelete) ewSetupAntiDelete();",
        )
        .replace(
            "ewSetupGhostMode();",
            "if (window.__wzConfig['incognito-noread'] || window.__wzConfig['incognito-notyping']) ewSetupGhostMode();",
        )
        .replace(
            "ewSetupJumpList();",
            "if (window.__wzConfig.jumpList) ewSetupJumpList();",
        );
    format!(
        "{}\nwindow.wzAppName = {app_name};\n{shim}",
        config_script(config)
    )
}

/// Wrap the embedded privacy stylesheet in a DOM style element update.
pub fn privacy_style_script() -> String {
    let css = serde_json::to_string(privacy::PRIVACY_CSS).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(function() {{
  let style = document.getElementById('whatszan-privacy-blur');
  if (!style) {{
    style = document.createElement('style');
    style.id = 'whatszan-privacy-blur';
    (document.head || document.documentElement).appendChild(style);
  }}
  style.textContent = {css};
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injection_gates_optional_features() {
        let script = injected_script("Test", &json!({ "jumpList": false }));
        assert!(script.contains("if (useAntiDelete) ewSetupAntiDelete();"));
        assert!(script.contains(
            "if (window.__wzConfig['incognito-noread'] || window.__wzConfig['incognito-notyping']) ewSetupGhostMode();"
        ));
        assert!(script.contains("if (window.__wzConfig.jumpList) ewSetupJumpList();"));
    }

    #[test]
    fn injection_rewrites_the_reserved_ipc_name() {
        let script = injected_script("Test", &json!({}));
        assert!(script.contains("window.__wzIpc"));
        assert!(!script.contains("window.ipc"));
    }

    #[test]
    fn config_script_installs_the_snapshot() {
        let script = config_script(&json!({ "jumpList": false }));
        assert!(script.contains("window.__wzConfig = config;"));
        assert!(script.contains("const config = {\"jumpList\":false};"));
    }

    #[test]
    fn config_script_propagates_debug_flag() {
        let script = config_script(&json!({ "debug": true }));
        assert!(script.contains("if (typeof config.debug === 'boolean') ipc.debug = config.debug;"));
        assert!(bridge::BRIDGE_JS.contains("ping: () => invoke(\"ping\")"));
    }
}
