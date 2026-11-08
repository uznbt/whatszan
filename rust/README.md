# WhatsZan Rust / Tauri rewrite

This directory contains a native Rust implementation of WhatsZan. It is a
standalone Tauri 2 + WebKitGTK application and does **not** run Electron or
Node.js at runtime.

The original Electron sources (`src/`, `src-web/`, `static/`, `locales/`,
`build/`, ...) are intentionally left untouched. Everything the Rust binary
needs at build time now lives under `rust/` (icons in
`crates/whatszan-app/assets/`, locales in `locales/`, and the debug page in
`crates/whatszan-app/frontend/`), so the Rust tree builds on its own. Both
front-ends still share the existing user profile and can coexist during the
migration.

## Status

Implemented:

- Main WhatsApp Web window with a Rust-owned feature configuration. The DOM
  shim (privacy blur, incognito helpers, keyboard shortcuts, custom app name,
  notification routing, share-file queue) is compiled into the binary from
  `renderer/injected.rs`; no `.js` or `.html` file is shipped inside `rust/`.
  Keybinding parsing and shared-file MIME detection also live in Rust
  (`whatszan-core`).
- Native GTK settings window built entirely in Rust (`settings_ui.rs`) with all
  tabs from the Electron page: general, privacy, dictionary, appearance,
  advanced, and branding. A custom `ToggleButton` sidebar drives a `GtkStack`
  (with a re-entrancy guard so programmatic tab activation cannot bounce
  between pages), and the page uses a themed header, cards, switches, entries,
  combos, sliders, a status toast, and light/dark palettes. It shares
  validation and side effects with the IPC `settings_save` command, auto-saves
  with a debounce, and rebuilds itself when the language changes so labels
  update immediately.
- Native application menu, tray icon (`ksni`, no libappindicator), D-Bus
  service (`org.uznbt.whatszan`) and Unity launcher badge. The tray title and
  tooltip use the configured custom app name.
- Native GTK message dialogs for About, session restore, configuration parse
  errors, and unsupported extensions. They use GTK directly instead of `rfd`'s
  portal/zenity fallback, so they work on desktops without `zenity`.
- Desktop notifications via XDG (`notify-rust`), including click-to-open.
- System theme detection through the XDG Settings portal with
  `gsettings` / KDE fallbacks.
- Encrypted config compatible with `src/json-config.mjs`
  (AES-256-GCM + scrypt), persistent state, and session snapshots. A parse
  error shows the Electron-style Continue/Quit dialog after the main window is
  created.
- Linux autostart, launcher entry, desktop shortcut, `whatsapp://` handler,
  and file-manager "Share with WhatsZan" actions. Shortcut repair follows
  Electron's `app-version` / `reapply-shortcuts.flag` logic, so normal launches
  do not rewrite the user's application entries.
- Smart downloads, custom user scripts/CSS, custom branding icons,
  spellcheck settings, WebKit preferred languages, and load-failure retry with
  an XDG notification (cancelled navigations are suppressed).
- Single-instance handling, `whatsapp://` links, `--share-media`,
  `--share-document`, `--open-chat=`, `--action=`, `--hide`, `--toggle`,
  `--quit`, and `--settings` (open the settings window directly).

## Requirements

- Rust stable (the workspace uses edition 2021).
- GTK 3 and WebKitGTK 4.1 development packages.
- `libsoup-3.0`, `dbus-1`, `pkg-config`, and a C toolchain.
- GStreamer base/good plugins for WhatsApp voice notes and calls, plus
  `gst-plugins-bad` (`webrtcbin`), `gst-libav` (H.264), and
  `gst-plugin-pipewire` (camera/screen capture). Without `autoaudiosink`
  WebKit logs `GStreamer element autoaudiosink not found`; the app still runs
  but audio playback may be silent.
- `libenchant` plus a dictionary backend (hunspell/nuspell) for spellcheck.
  Missing backends only produce `libenchant-WARNING` messages.
- `xdg-mime` and `update-desktop-database` for desktop integration
  (optional; failures are logged and ignored).
- `zip` for sharing directories as documents (optional).

On Arch-based systems the native packages are typically:

```sh
sudo pacman -S --needed base-devel pkgconf gtk3 webkit2gtk-4.1 libsoup3 dbus
```

Debian/Ubuntu names are usually `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`,
`libsoup-3.0-dev`, and `libdbus-1-dev`.

## Build and run

```sh
source "$HOME/.cargo/env"
cd rust

# Fast compile check
cargo check -p whatszan

# Platform-independent logic (26 tests)
cargo test -p whatszan-core

# Embedded renderer/config and asset tests (8 tests)
cargo test -p whatszan

# Debug binary
cargo build -p whatszan
./target/debug/whatszan

# Release binary
cargo build --release -p whatszan
./target/release/whatszan
```

The Tauri CLI is not required for a plain `cargo build`. To produce a bundle,
install `cargo-tauri` and run `cargo tauri build` from
`rust/crates/whatszan-app` after reviewing `tauri.conf.json`.

Useful runtime flags:

```sh
./target/debug/whatszan            # normal start
./target/debug/whatszan --hide     # start hidden (tray only)
./target/debug/whatszan --settings # settings only; stays open until closed
./target/debug/whatszan --toggle   # toggle an existing instance
./target/debug/whatszan --quit     # ask an existing instance to exit
```

### Debug mode

`DEBUG=1 ./target/debug/whatszan` loads the Rust-owned
`crates/whatszan-app/frontend/debug.html`, opens WebKit developer tools, and
matches the Electron `DEBUG=1` behaviour. The normal `open-dev-tools` config
key also works.

The debug page has no external script: Rust installs the equivalent `ping`,
`notify`, and `but1` handlers against `window.__wzIpc` after the page reports
ready, and the bridge's `debug` flag is set from the Rust configuration
snapshot. It is the only frontend page loaded through Tauri's `frontendDist`;
the normal settings UI is native GTK and WhatsApp itself is remote.

## Asset and profile locations

The Rust binary is self-contained for its normal pages and artwork:

- Default icons live in `crates/whatszan-app/assets/` and are embedded at
  compile time by `crates/whatszan-app/src/assets.rs`.
- Locale catalogs live in `rust/locales/` and are embedded by
  `whatszan-core/src/translations.rs`. `WHATZAN_LOCALES_DIR` remains an
  optional filesystem override, which is useful while editing translations.
- Renderer JavaScript and privacy CSS come from the embedded
  `crates/whatszan-app/src/renderer/` modules. The Electron `src-web/` files
  are not read at runtime.
- The debug page lives in `crates/whatszan-app/frontend/debug.html` and is
  embedded through Tauri's `frontendDist` at build time; it is only used by
  `DEBUG=1`.
- User data comes from `~/.config/WhatsZan` (or `WHATZAN_USER_DATA`).
- The WebKit profile lives in `<userData>/webkit`.
- Tauri bundle icons live in `rust/crates/whatszan-app/icons/` and must be
  RGBA PNGs; Tauri rejects palette-alpha icons.

Override paths when packaging or testing:

```sh
WHATZAN_USER_DATA=/tmp/whatszan-profile
WHATZAN_LOCALES_DIR=/usr/share/whatszan/locales
```

## Migration notes

- The encrypted `config.json` format is intentionally compatible with the
  Electron client. Existing settings should load without conversion.
- WebKitGTK cannot read Chromium's IndexedDB/Local Storage. A WhatsApp QR scan
  is required once when switching from the Electron build. Session snapshots
  include both Chromium and WebKit directory names so switching back and forth
  does not destroy either profile.
- Close the Electron client before the first Rust run if both use the same
  `~/.config/WhatsZan` directory. The config files are compatible, but
  concurrent writes are not coordinated.
- `--hide` starts hidden regardless of `DEBUG`/`quit-on-close`, matching
  Electron's `show: isHiddenStartup ? false : ...`.
- `--settings` keeps the main window hidden and leaves the settings window open
  until the user closes it. Openings from the menu/tray keep the Electron
  popup behavior and close on focus loss; native file choosers temporarily
  suppress that via `settings_ignore_blur`.
- WebKit occasionally cancels the first WhatsApp navigation
  (`Operation was cancelled`) in this environment and then leaves an empty
  document. The `load-failed` handler re-navigates to the configured URL after
  `retry-interval` (default 15 s) unless the page already became ready, so the
  app recovers without user action. Real failures also send a "Failed to load"
  notification; cancellation errors are suppressed to avoid retry noise.
- WebKit's `Accept-Language` and spellcheck languages follow Electron:
  `app-language` (plus `en` when explicitly set) drives preferred languages,
  and an unset `spellcheck-languages` falls back to the system preference list.
- If `config.json` cannot be parsed, the Rust build starts with defaults and
  shows a Continue/Quit dialog, matching Electron's `config.parseError`
  handling. Persistent-state parse errors are logged but do not block startup.
- The renderer API is exposed as `window.__wzIpc`, not `window.ipc`. WRY
  reserves `window.ipc` on Linux for Tauri's own frozen `postMessage` channel,
  so the embedded shim is rewritten while Rust composes the injection script.
- Rust injects `window.__wzConfig` before the shim runs. The shim's
  `stateGet` calls resolve from that snapshot, and settings changes push a
  fresh snapshot into the live page. Feature setup (anti-delete, ghost mode,
  status saver, jump list) is gated by Rust decisions, not started blindly.
- Remote WhatsApp pages may call the app's commands only because
  `build.rs` generates an application ACL manifest and
  `permissions/default.toml` allows each command in the `default` capability.
  Add new IPC commands to all three places: `generate_handler!`, the
  `build.rs` list, and `permissions/default.toml`.
- `window.open()` targets that belong to WhatsApp (`about:blank`, `blob:`,
  `data:`, `web.whatsapp.com`, and `*.whatsapp.net`) open in a real Tauri popup
  window. The popup shares the opener's WebKit context and receives the same
  Electron user agent, `navigator` shim, IPC bridge, media settings, and
  auto-allow permission handler. External URLs are sent to the desktop browser,
  matching Electron's `setWindowOpenHandler`. The capability file uses the
  `popup-*` glob so those windows can reach the renderer bridge.
- Every WebKit permission request is granted (`request.allow()`), matching
  Electron's `setPermissionRequestHandler`/`setPermissionCheckHandler`:
  notifications, microphone, camera, display capture, clipboard, and device
  info. Popups inherit the same handler and media settings.
- Permission *checks* report `granted` immediately, like Electron's
  `setPermissionCheckHandler`. The init script overrides
  `Notification.permission`, `Notification.requestPermission()`, and
  `navigator.permissions.query()` for notifications, push, microphone, camera,
  display capture, and clipboard. Without this, WhatsApp shows the
  "message and call notifications are off" banner even though requests would
  be allowed.
- Page-level notifications go through the injected `Notification` proxy. As a
  fallback, WebKitGTK's `show-notification` signal (used by service-worker
  notifications) is routed to the same XDG backend and honours Do Not Disturb.
- Electron's legacy `accounts-get`, `active-account-get`, and `account-remove`
  handlers were constants/no-ops and were never exposed by the preload, so the
  Rust build intentionally omits them.

## Known limitations / differences

- **Chrome extensions are not supported.** WebKitGTK has no Chromium extension
  API. The settings page explains this and links to the Chrome Web Store;
  `install-extension` is a no-op that reports the limitation.
- **Notification inline replies are not supported.** XDG desktop notifications
  have no portable reply-text action. Clicking the notification opens the chat;
  `notify-reply` is never emitted.
- **Menu auto-hide is approximated.** GTK has no Electron-style auto-hiding
  menu bar. When `menu-bar-auto-hide` is enabled the menu starts hidden and
  Alt toggles it.
- **Anti-screencast is best-effort.** `set_content_protected` is called, but
  WebKitGTK/GTK does not provide Chromium's content protection on Linux.
- **`disable-gpu` is startup-only.** It sets
  `WEBKIT_DISABLE_COMPOSITING_MODE=1` and
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` before GTK starts. Restart the app after
  changing it.
- **`low-resource-mode` is a no-op.** The Chromium background-network switches
  do not exist in WebKitGTK.
- **Screen sharing uses the desktop portal.** WebKitGTK's `getDisplayMedia()`
  shows the portal's own picker; the custom `static/screen-picker.html` flow is
  not wired to the Rust build.
- **Recent-chat jump lists are disabled on Linux.** They are a Windows shell
  feature; Rust reports `jumpList: false` so the shim skips the scan entirely.
- **`whatsapp://` registration depends on the desktop environment.** The Rust
  build writes `MimeType=x-scheme-handler/whatsapp` and calls `xdg-mime
  default WhatsZan.desktop x-scheme-handler/whatsapp`. Some desktops require a
  logout or a manual default-app refresh before the change takes effect.
- **WhatsApp Web compatibility is not guaranteed.** The default Electron-style
  user agent is preserved (including the `Electron/<version>` token, so linked
  devices are reported as a desktop app rather than Chrome/Safari), but
  WebKitGTK is not Chromium. If WhatsApp changes its browser checks, the user
  agent can be overridden in settings.

## Architecture

```
rust/
  Cargo.toml                 workspace
  crates/whatszan-core/      platform-independent logic
    config.rs                encrypted JSON config (Node-compatible)
    keys.rs                  default keybindings + binding parser
    mime.rs                  MIME type detection for shared files
    paths.rs                 profile/asset locations
    session_backup.rs        snapshot create/restore/retention
    shortcuts.rs             autostart, launcher, share actions, URL handler
    translations.rs          locale loading
    url.rs                   WhatsApp URL helpers
  crates/whatszan-app/       Tauri GUI/backend
    main.rs                  CLI, plugins, setup, signals
    assets.rs                embedded default icons
    app_state.rs             shared state + Rust-owned renderer config
    webview.rs               main window, injection, downloads, retry
    settings_ui.rs           native GTK settings window
    commands.rs              Tauri IPC commands + shared settings logic
    menu.rs                  native application menu
    tray.rs                  StatusNotifierItem tray
    dbus.rs                  D-Bus service and Unity badge
    dialogs.rs               native GTK message dialogs
    notifications.rs         XDG notifications
    theme.rs                 portal/gsettings/KDE theme detection
    renderer/mod.rs          composes config + shim for injection
    renderer/bridge.rs       embedded Tauri/Electron IPC bridge
    renderer/injected.rs     embedded WhatsApp DOM shim
    renderer/privacy.rs      embedded privacy-blur CSS
    icons/                   Tauri bundle icons (RGBA PNG)
```

The renderer contract is the same as the Electron preload:

- `window.__wzIpc.*` methods are backed by Tauri commands. The name differs
  from Electron because WRY owns `window.ipc` on Linux.
- The embedded bridge is a Tauri initialization script. It reports
  `page_ready` from the real document (on `load`, with a 15 s fallback timer),
  and Rust then injects the embedded DOM shim. WebKit's
  `PageLoadEvent::Finished` can fire while the JavaScript context is still
  `about:blank`, so it is not used as the injection trigger.
- Rust parses keyboard bindings into `__wzConfig.keyBindings`; the shim only
  compares them with DOM keyboard events and maps digit keys for non-QWERTY
  layouts. Shared-file MIME types come from `whatszan-core::mime`, not from a
  JavaScript extension table.
- Privacy CSS and user CSS are inserted as `<style>` elements.
- `settings-saved` is still emitted to all windows for renderer-side reloads;
  the native settings window calls Rust directly and does not need it.
- The GTK settings window is transient for the main window and closes when it
  loses focus, except while a native file/folder chooser is open. Changing the
  language closes and rebuilds it from the new translations.

## Verification

```sh
source "$HOME/.cargo/env"
cd rust
cargo test -p whatszan-core   # 25 platform-independent tests
cargo test -p whatszan        # 4 renderer-assembly + 3 language-normalization tests
cargo check -p whatszan
```

For a safe smoke test that does not touch the live profile or desktop entries:

```sh
tmp=$(mktemp -d)
mkdir -p "$tmp/profile"
printf '%s\n' '{"dbus":false,"quit-on-close":true}' > "$tmp/profile/config.json"
HOME="$tmp" WHATZAN_USER_DATA="$tmp/profile" \
  GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 RUST_LOG=info \
  timeout 75s ./target/debug/whatszan --hide
```

Allow 45–75 seconds: tray registration and the WhatsApp retry can both take a
while. A successful run logs `[bridge] IPC ready on web.whatsapp.com`,
`renderer page_ready`, and `injecting WhatsApp integration`.

To exercise the settings window instead:

```sh
HOME="$tmp" WHATZAN_USER_DATA="$tmp/profile" \
  GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 RUST_LOG=info \
  timeout 45s ./target/debug/whatszan --settings
```

The settings window is a native GTK window titled `Pengaturan`; it is created
on the GTK main thread and shares `commands::save_settings` with the IPC path.
No `settings.html` or `settings-bridge.js` is loaded. With AT-SPI enabled,
toggling a switch, editing the notification prefix, or adding a dictionary
entry can be verified by decrypting the resulting `config.json` /
`persistent-state.json` with the same scrypt key described in
`whatszan-core::config`.

(Close the Electron client first if you want to test against the real profile.)

## License

GPL-3.0-or-later, matching the original project.
