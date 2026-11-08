//! Tauri commands exposed to `window.ipc` (the renderer bridge) and to the
//! native GTK settings window.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use whatszan_core::{
    mime_for_name,
    shortcuts::{stable_exe_path, DesktopIntegration},
};

use crate::{
    app_state::{AppState, ShareRequest},
    menu, notifications, tray, webview,
};

/// Desktop integration writes shared files and calls `xdg-mime`. Serialize
/// every invocation so background branding updates cannot race the settings
/// window or another icon refresh.
static DESKTOP_INTEGRATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Serialize)]
pub struct FileBuffer {
    pub data: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
}

#[derive(Debug, Deserialize)]
pub struct BrandingPayload {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
}

#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
pub fn state_get(name: String, state: State<'_, AppState>) -> Value {
    state.state_value(&name)
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Value {
    state.settings_value()
}

#[tauri::command]
pub fn settings_get_translations(state: State<'_, AppState>) -> Value {
    Value::Object(state.reload_translations())
}

#[tauri::command]
pub fn window_toggle(app: AppHandle) -> bool {
    webview::toggle_main_window(&app)
}

#[tauri::command]
pub fn escape_pressed(app: AppHandle) {
    let state = app.state::<AppState>();
    let enabled = state
        .config
        .read()
        .unwrap()
        .get_bool("esc-toggle-window", false);
    if enabled {
        let _ = webview::toggle_main_window(&app);
    }
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    webview::open_settings(&app);
}

#[tauri::command]
pub fn toggle_menu(app: AppHandle) {
    menu::toggle_menu(&app);
}

#[tauri::command]
pub fn toggle_privacy_blur(app: AppHandle) -> bool {
    let state = app.state::<AppState>();
    let current = state
        .persist
        .read()
        .unwrap()
        .get_bool("privacy-blur", false);
    let next = !current;
    {
        let mut persist = state.persist.write().unwrap();
        let _ = persist.set("privacy-blur", json!(next));
    }
    if let Some(window) = state.main_window() {
        webview::apply_blur_state(&app, &window);
        webview::show_toast(
            &window,
            if next {
                "Privacy Blur: ON"
            } else {
                "Privacy Blur: OFF"
            },
        );
    }
    tracing::debug!(next, "privacy blur toggled");
    next
}

#[tauri::command]
pub fn toggle_anti_screencast(app: AppHandle) -> bool {
    let state = app.state::<AppState>();
    let current = state
        .persist
        .read()
        .unwrap()
        .get_bool("anti-screencast", false);
    let next = !current;
    {
        let mut persist = state.persist.write().unwrap();
        let _ = persist.set("anti-screencast", json!(next));
    }
    if let Some(window) = state.main_window() {
        // WebKitGTK does not implement Chromium's content protection. The
        // call is harmless and does the right thing on platforms that support
        // it; on Linux the setting remains a best-effort flag.
        let _ = window.set_content_protected(next);
        webview::show_toast(
            &window,
            if next {
                "Anti Screen Cast: ON"
            } else {
                "Anti Screen Cast: OFF"
            },
        );
    }
    let _ = app.emit("settings-saved", ());
    next
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) {
    tracing::debug!(%url, "open URL");
    webview::handle_open_url(&app, &url);
}

#[tauri::command]
pub fn notify_ev(app: AppHandle, args: Value) {
    tracing::debug!(?args, "notification event");
    webview::show_main_window(&app);
}

#[tauri::command]
pub fn notify_send(
    app: AppHandle,
    id: String,
    title: String,
    body: String,
    icon_data_url: Option<String>,
) {
    let state = app.state::<AppState>();
    if state.is_dnd() {
        tracing::debug!(%title, "[DND] notification blocked");
        return;
    }
    notifications::send(app.clone(), id, title, body, icon_data_url);
}

#[tauri::command]
pub fn log_to_main(msg: String) {
    tracing::info!(target: "renderer", "{msg}");
}

/// Called by the renderer bridge from the document's `load` event.
///
/// WebKit can emit `PageLoadEvent::Finished` while its JavaScript context is
/// still the previous `about:blank` document, which made `eval` land in the
/// wrong frame. The page itself is the reliable signal, so the bridge asks us
/// to inject after the real document exists.
#[tauri::command]
pub fn page_ready(webview: tauri::WebviewWindow, app: AppHandle, url: String) {
    tracing::info!(%url, label = webview.label(), "renderer page_ready");
    let is_whatsapp = whatszan_core::is_web_whatsapp(&url);
    let is_local = url.starts_with("tauri://") || url.starts_with("http://tauri.localhost");
    if !is_whatsapp && !is_local {
        tracing::debug!(%url, "ignoring page_ready for foreign origin");
        return;
    }

    // A window.open() child also runs the bridge; it must not flip the main
    // window's ready state or inject WhatsApp integration into the main page.
    let Some(main_window) = webview::main_window(&app) else {
        return;
    };
    if webview.label() != main_window.label() {
        tracing::debug!(
            %url,
            label = webview.label(),
            "ignoring page_ready from popup window"
        );
        return;
    }

    let state = app.state::<AppState>();
    if state.is_main_page_ready() || state.is_quitting() {
        tracing::debug!(%url, "page_ready already handled");
        return;
    }
    state.set_main_page_ready(true);
    state.set_main_page_loaded(true);

    webview::inject_page_scripts(main_window);
    webview::emit_pending_events(&app);
}

#[tauri::command]
pub fn insert_text(app: AppHandle, text: String) {
    let Some(window) = webview::main_window(&app) else {
        return;
    };
    let text = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
    let _ = window.eval(format!(
        "try {{ document.execCommand('insertText', false, {text}); }} catch (e) {{}}"
    ));
}

#[tauri::command]
pub fn update_recent_chats(_chats: Vec<String>) {
    // Jump lists are a Windows shell feature; the Linux build has no direct
    // equivalent. Keep the IPC method available so injected.js stays happy.
}

#[tauri::command]
pub fn update_unread(app: AppHandle, favicon: String) -> u32 {
    let unread = whatszan_core::unread_count_from_favicon(&favicon);
    let state = app.state::<AppState>();
    let previous = state.set_unread(unread);

    if let Some(window) = state.main_window() {
        let badge = if unread > 0 {
            Some(unread as i64)
        } else {
            None
        };
        let _ = window.set_badge_count(badge);
    }

    if previous != unread {
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tray::set_unread(&app_handle, unread).await;
            crate::dbus::set_badge_count(&app_handle, unread).await;
        });
    }

    unread
}

#[tauri::command]
pub fn get_pending_share(state: State<'_, AppState>) -> Option<ShareRequest> {
    state.pending_share.lock().unwrap().take()
}

#[tauri::command]
pub fn read_file_buffer(path: String) -> Option<FileBuffer> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return None;
    }
    match fs::read(&path) {
        Ok(bytes) => {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file")
                .to_string();
            let mime = mime_for_name(&name).to_string();
            Some(FileBuffer {
                data: BASE64.encode(bytes),
                name,
                mime,
            })
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "cannot read shared file");
            None
        }
    }
}

#[tauri::command]
pub async fn browse_folder(app: AppHandle) -> Option<String> {
    let state = app.state::<AppState>();
    state.set_settings_ignore_blur(true);
    let folder = rfd::AsyncFileDialog::new().pick_folder().await;
    state.set_settings_ignore_blur(false);
    folder.map(|handle| handle.path().display().to_string())
}

#[tauri::command]
pub fn get_default_download_paths() -> Value {
    json!({
        "image": dirs::picture_dir().unwrap_or_default().display().to_string(),
        "video": dirs::video_dir().unwrap_or_default().display().to_string(),
        "audio": dirs::audio_dir().unwrap_or_default().display().to_string(),
        "document": dirs::document_dir().unwrap_or_default().display().to_string(),
    })
}

#[tauri::command]
pub fn check_path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
pub fn settings_ignore_blur(app: AppHandle, ignore: bool) {
    app.state::<AppState>().set_settings_ignore_blur(ignore);
}

#[tauri::command]
pub fn open_webstore() {
    if let Err(err) = open::that("https://chromewebstore.google.com/") {
        tracing::warn!(%err, "cannot open web store");
    }
}

#[tauri::command]
pub async fn install_extension(app: AppHandle, extension_id: String) -> bool {
    tracing::warn!(
        %extension_id,
        "Chrome extensions cannot be installed in WebKitGTK; ignoring request"
    );
    let _ = app;
    crate::dialogs::message(
        crate::dialogs::Level::Error,
        "Ekstensi tidak didukung",
        "WhatsZan versi Rust memakai WebKitGTK, yang tidak mendukung ekstensi Chrome. \
         Fitur ekstensi hanya tersedia pada versi Electron.",
        crate::dialogs::Buttons::ok("OK"),
    )
    .await;
    false
}

#[tauri::command]
pub fn screen_share_select(_source_id: Option<String>) -> bool {
    // WebKitGTK hands getDisplayMedia() to the desktop portal's own picker.
    tracing::debug!("screen picker selection ignored (portal handles capture)");
    false
}

#[tauri::command]
pub fn settings_save(app: AppHandle, new_settings: Value) -> Result<(), String> {
    save_settings(&app, new_settings)
}

/// Persist a settings object. Shared by the IPC command and the native GTK
/// settings window, so both frontends apply exactly the same side effects.
pub fn save_settings(app: &AppHandle, new_settings: Value) -> Result<(), String> {
    let mut settings = new_settings
        .as_object()
        .cloned()
        .ok_or_else(|| "settings-save expects an object".to_string())?;

    let blur = take_bool(&mut settings, "privacy-blur", false);
    let anti_screencast = take_bool(&mut settings, "anti-screencast", false);
    let auto_run = take_bool(&mut settings, "auto-run", false);
    let desktop_shortcut = take_bool(&mut settings, "desktop-shortcut", true);
    let sidebar_width = settings
        .remove("sidebar-width")
        .and_then(|value| value.as_u64())
        .unwrap_or(220);
    let new_app_name = settings
        .remove("custom-app-name")
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "WhatsZan".into());
    let new_app_name = if new_app_name.trim().is_empty() {
        "WhatsZan".to_string()
    } else {
        new_app_name.trim().to_string()
    };

    let state = app.state::<AppState>();
    let (old_auto_run, old_desktop_shortcut, old_app_name) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_bool("auto-run", false),
            persist.get_bool("desktop-shortcut", true),
            persist.get_str("custom-app-name", "WhatsZan").to_string(),
        )
    };
    let old_app_name = if old_app_name.trim().is_empty() {
        "WhatsZan".to_string()
    } else {
        old_app_name
    };

    {
        let mut persist = state.persist.write().unwrap();
        persist
            .update(|data| {
                data.insert("privacy-blur".into(), json!(blur));
                data.insert("anti-screencast".into(), json!(anti_screencast));
                data.insert("auto-run".into(), json!(auto_run));
                data.insert("desktop-shortcut".into(), json!(desktop_shortcut));
                data.insert("custom-app-name".into(), json!(new_app_name));
                data.insert("sidebar-width".into(), json!(sidebar_width));
            })
            .map_err(|err| err.to_string())?;
    }

    {
        let mut config = state.config.write().unwrap();
        config
            .update(|data| {
                for (key, value) in settings {
                    let empty =
                        value.is_null() || value.as_str().is_some_and(|text| text.is_empty());
                    if empty {
                        data.remove(&key);
                    } else {
                        data.insert(key, value);
                    }
                }
            })
            .map_err(|err| err.to_string())?;
    }

    if old_auto_run != auto_run {
        apply_auto_run(app, auto_run, false);
    }
    if old_desktop_shortcut != desktop_shortcut
        || (desktop_shortcut && old_app_name != new_app_name)
    {
        apply_desktop_shortcut(app);
    }

    // Reload translations before rebuilding the menu and tray so both pick up
    // a language change from the same save.
    state.reload_translations();

    if let Some(window) = state.main_window() {
        webview::apply_renderer_config(&window, &state);
        webview::apply_blur_state(app, &window);
        webview::update_user_agent(&window, &state.user_agent());
        webview::apply_spellcheck(&window);
        let _ = window.set_content_protected(anti_screencast);
        let _ = window.set_title(&new_app_name);
        if old_app_name != new_app_name {
            apply_app_name(&window, &old_app_name, &new_app_name);
        }
        menu::apply_menu(app, &window);
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tray::refresh(&app_handle).await;
    });

    let _ = app.emit("settings-saved", ());
    Ok(())
}

fn take_bool(map: &mut Map<String, Value>, key: &str, default: bool) -> bool {
    map.remove(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn apply_app_name(window: &tauri::WebviewWindow, old_name: &str, new_name: &str) {
    let old_name = serde_json::to_string(old_name).unwrap_or_else(|_| "\"\"".into());
    let new_name = serde_json::to_string(new_name).unwrap_or_else(|_| "\"\"".into());
    let script = format!(
        r#"(function() {{
  var newName = {new_name};
  var oldName = {old_name};
  window.wzAppName = newName;

  const processTextNode = (node) => {{
    if (node.nodeValue === oldName || node.nodeValue === oldName + ' Web' || node.nodeValue === 'WhatsApp' || node.nodeValue === 'WhatsApp Web') {{
      let parent = node.parentElement;
      let isMessage = false;
      while (parent) {{
        if (parent.getAttribute && (parent.getAttribute('data-testid') === 'msg-container' || parent.getAttribute('role') === 'row' || parent.id === 'main')) {{
          isMessage = true;
          break;
        }}
        parent = parent.parentElement;
      }}
      if (!isMessage) node.nodeValue = newName;
    }}
  }};

  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null, false);
  let node;
  while ((node = walker.nextNode())) processTextNode(node);

  if (newName.toLowerCase() === 'whatsapp') {{
    document.querySelectorAll('.whatszan-logo').forEach(el => el.remove());
    document.querySelectorAll('svg[aria-label="WhatsApp"]').forEach(svg => svg.style.display = '');
    document.title = 'WhatsApp Web';
  }} else {{
    document.querySelectorAll('.whatszan-logo').forEach(el => el.textContent = newName);
    document.querySelectorAll('svg[aria-label="WhatsApp"]').forEach(svg => {{
      svg.style.display = 'none';
      if (svg.parentElement && !svg.parentElement.querySelector('.whatszan-logo')) {{
        const span = document.createElement('span');
        span.className = 'whatszan-logo';
        span.textContent = newName;
        span.style.fontWeight = 'bold';
        span.style.fontSize = '18px';
        span.style.color = 'inherit';
        svg.parentElement.appendChild(span);
      }}
    }});
    if (document.title.includes(oldName) || document.title.includes('WhatsApp')) {{
      document.title = newName;
    }}
  }}
}})();"#
    );
    let _ = window.eval(script);
}

fn desktop_integration() -> DesktopIntegration {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let desktop = dirs::desktop_dir().unwrap_or_else(|| home.join("Desktop"));
    DesktopIntegration::new(home, desktop, stable_exe_path())
}

/// Write the current app branding to a stable-per-content PNG and return its
/// path. `.desktop` files cannot embed the image, and icon names such as
/// `whatszan` resolve to the package artwork, which never follows branding.
fn resolve_desktop_icon(state: &AppState) -> Result<PathBuf, String> {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let (choice, custom) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_str("icon-choice-app", "whatszan").to_string(),
            persist.get_str("custom_tray_app", "").to_string(),
        )
    };

    let mut bytes = match choice.as_str() {
        "custom" if !custom.is_empty() => {
            fs::read(&custom).map_err(|err| format!("cannot read {custom}: {err}"))?
        }
        "whatsapp" => crate::assets::WHATSAPP_ICON_PNG.to_vec(),
        _ => crate::assets::APP_ICON_PNG.to_vec(),
    };

    let image = match image::load_from_memory(&bytes) {
        Ok(image) => image,
        Err(err) if choice == "custom" => {
            tracing::warn!(%err, "cannot decode custom app icon; using default");
            bytes = crate::assets::APP_ICON_PNG.to_vec();
            image::load_from_memory(&bytes).map_err(|err| format!("embedded app.png: {err}"))?
        }
        Err(err) => return Err(format!("cannot decode app icon: {err}")),
    };

    // Content-addressed name: changing the artwork changes the file name, so
    // launchers that cache by path notice the update.
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let destination = state
        .user_data
        .join(format!("app-icon-{:016x}.png", hasher.finish()));

    if !destination.exists() {
        image
            .resize(512, 512, image::imageops::FilterType::Lanczos3)
            .save(&destination)
            .map_err(|err| format!("cannot save {}: {err}", destination.display()))?;
    }

    // Remove artwork from previous choices; entries never point at it again.
    if let Ok(entries) = fs::read_dir(&state.user_data) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == destination {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("app-icon-") && name.ends_with(".png") {
                let _ = fs::remove_file(path);
            }
        }
    }

    Ok(destination)
}

/// Create the desktop entry named after the window's app id when it is
/// missing or still points at an older branding. Upgrades from versions that
/// only wrote `WhatsZan.desktop` rely on this to fix the dock icon without
/// requiring the user to change settings first.
fn ensure_app_desktop_entry(app: &AppHandle) {
    let state = app.state::<AppState>();
    let integration = desktop_integration();
    let app_id = integration.app_id();
    let path = integration.app_desktop_path();

    let name = {
        let persist = state.persist.read().unwrap();
        let name = persist
            .get_str("custom-app-name", "WhatsZan")
            .trim()
            .to_string();
        if name.is_empty() {
            "WhatsZan".to_string()
        } else {
            name
        }
    };
    let Ok(icon) = resolve_desktop_icon(&state) else {
        apply_branding_shortcut(app);
        return;
    };
    let icon = icon.display().to_string();

    let current = fs::read_to_string(&path).unwrap_or_default();
    let up_to_date = current.contains(&format!("\nName={name}\n"))
        && current.contains(&format!("\nIcon={icon}\n"))
        && current.contains(&format!("\nStartupWMClass={app_id}\n"));
    if !up_to_date {
        apply_branding_shortcut(app);
    }
}

fn apply_auto_run(app: &AppHandle, auto_run: bool, startup_run: bool) {
    if let Err(err) = desktop_integration().apply_auto_run(auto_run, startup_run) {
        tracing::warn!(%err, "cannot update autostart entry");
    }
    let _ = app;
}

fn apply_desktop_shortcut(app: &AppHandle) {
    apply_desktop_integration(app, true);
}

/// Branding changes only need to rewrite the icon in the `.desktop` entry.
/// Registering the URL handler again calls `xdg-mime` and is unnecessary (and
/// can block on broken desktop setups).
fn apply_branding_shortcut(app: &AppHandle) {
    apply_desktop_integration(app, false);
}

fn apply_desktop_integration(app: &AppHandle, register_url_handler: bool) {
    let _guard = DESKTOP_INTEGRATION_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());

    let state = app.state::<AppState>();
    let register_scheme = state
        .config
        .read()
        .unwrap()
        .get_bool("register-url-scheme", true);
    let (create, name, translations) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_bool("desktop-shortcut", true),
            persist
                .get_str("custom-app-name", "WhatsZan")
                .trim()
                .to_string(),
            state.translations(),
        )
    };
    let name = if name.is_empty() {
        "WhatsZan".to_string()
    } else {
        name
    };

    // Desktop entries cannot reference the embedded artwork, so materialize
    // the current choice as an ordinary PNG and point the entries at it.
    let icon_location = match resolve_desktop_icon(&state) {
        Ok(path) => path.display().to_string(),
        Err(err) => {
            tracing::warn!(%err, "cannot prepare branded desktop icon");
            "whatszan".to_string()
        }
    };

    let integration = desktop_integration();
    if let Err(err) =
        integration.apply_desktop_shortcut(create, &name, &name, &icon_location, &translations)
    {
        tracing::warn!(%err, "cannot update desktop shortcut");
    }
    if register_url_handler && register_scheme {
        if let Err(err) = integration.register_url_handler() {
            tracing::debug!(%err, "cannot register whatsapp:// URL handler");
        }
    }
}

#[tauri::command]
pub fn branding_upload_icon(app: AppHandle, payload: BrandingPayload) -> Result<bool, String> {
    upload_branding_path(&app, &payload.kind, &payload.path)?;
    Ok(true)
}

/// Shared by the IPC command and the native GTK icon picker.
pub fn upload_branding_path(app: &AppHandle, kind: &str, path: &str) -> Result<(), String> {
    let kind = valid_branding_kind(kind)?;
    let state = app.state::<AppState>();

    if path == "whatszan" {
        remove_custom_icon(&state, kind);
        set_icon_choice(&state, kind, "whatszan");
        after_branding_change(app, kind);
        return Ok(());
    }

    let image = if path == "whatsapp" {
        image::load_from_memory(crate::assets::WHATSAPP_ICON_PNG)
            .map_err(|err| format!("embedded whatsapp.png: {err}"))?
    } else {
        let source = PathBuf::from(path);
        image::open(&source).map_err(|err| format!("{}: {err}", source.display()))?
    };
    let destination = save_branding_image(&state, kind, image)?;
    let choice = if path == "whatsapp" {
        "whatsapp"
    } else {
        "custom"
    };
    persist_custom_icon(&state, kind, &destination, choice);
    after_branding_change(app, kind);
    Ok(())
}

#[tauri::command]
pub fn branding_upload_bytes(
    app: AppHandle,
    kind: String,
    _name: String,
    data: String,
) -> Result<bool, String> {
    upload_branding_bytes(&app, &kind, &data)?;
    Ok(true)
}

/// Shared by the IPC command and any future native upload UI.
pub fn upload_branding_bytes(app: &AppHandle, kind: &str, data: &str) -> Result<(), String> {
    let kind = valid_branding_kind(kind)?;
    let encoded = data
        .split_once(',')
        .filter(|(prefix, _)| prefix.starts_with("data:"))
        .map(|(_, payload)| payload)
        .unwrap_or(data);
    let bytes = BASE64
        .decode(encoded)
        .map_err(|err| format!("invalid base64 image: {err}"))?;
    let image = image::load_from_memory(&bytes).map_err(|err| format!("invalid image: {err}"))?;

    let state = app.state::<AppState>();
    let destination = save_branding_image(&state, kind, image)?;
    persist_custom_icon(&state, kind, &destination, "custom");
    after_branding_change(app, kind);
    Ok(())
}

#[tauri::command]
pub fn branding_reset_icons(app: AppHandle) -> bool {
    reset_branding_icons(&app)
}

/// Shared by the IPC command and the native GTK settings window.
pub fn reset_branding_icons(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    {
        let mut persist = state.persist.write().unwrap();
        for kind in ["app", "unread", "normal"] {
            let _ = persist.delete(&format!("custom_tray_{kind}"));
            let _ = persist.set(format!("icon-choice-{kind}"), json!("whatszan"));
        }
    }

    if let Some(window) = state.main_window() {
        webview::set_window_icon(&window);
    }
    let app_handle = app.clone();
    std::thread::spawn(move || apply_branding_shortcut(&app_handle));
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tray::refresh(&app_handle).await;
    });
    true
}

fn valid_branding_kind(kind: &str) -> Result<&str, String> {
    match kind {
        "app" | "unread" | "normal" => Ok(kind),
        other => Err(format!("unknown icon type: {other}")),
    }
}

fn save_branding_image(
    state: &AppState,
    kind: &str,
    image: image::DynamicImage,
) -> Result<PathBuf, String> {
    let size = if kind == "app" { 256 } else { 32 };
    let resized = image
        .resize(size, size, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let destination = state.user_data.join(format!("custom_tray_{kind}.png"));
    resized
        .save(&destination)
        .map_err(|err| format!("cannot save {}: {err}", destination.display()))?;
    Ok(destination)
}

fn persist_custom_icon(state: &AppState, kind: &str, path: &Path, choice: &str) {
    let mut persist = state.persist.write().unwrap();
    let _ = persist.set(
        format!("custom_tray_{kind}"),
        json!(path.display().to_string()),
    );
    let _ = persist.set(format!("icon-choice-{kind}"), json!(choice));
}

fn set_icon_choice(state: &AppState, kind: &str, choice: &str) {
    let mut persist = state.persist.write().unwrap();
    let _ = persist.set(format!("icon-choice-{kind}"), json!(choice));
}

fn remove_custom_icon(state: &AppState, kind: &str) {
    let mut persist = state.persist.write().unwrap();
    let _ = persist.delete(&format!("custom_tray_{kind}"));
}

fn after_branding_change(app: &AppHandle, kind: &str) {
    let state = app.state::<AppState>();
    if kind == "app" {
        if let Some(window) = state.main_window() {
            webview::set_window_icon(&window);
        }
        // Rewriting the `.desktop` file touches shared files and helper
        // commands. Keep that off the GTK main thread so a stuck helper
        // cannot freeze the settings window.
        let app_handle = app.clone();
        std::thread::spawn(move || apply_branding_shortcut(&app_handle));
    }
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tray::refresh(&app_handle).await;
    });
}

/// Re-apply desktop integration at startup, following Electron's
/// `app-version` / `reapply-shortcuts.flag` logic.
///
/// `applyAutoRun` runs on every start (so a missing autostart file is repaired
/// when the setting is enabled), but the `.desktop` shortcut and URL handler
/// are only rewritten after an upgrade or when the repair flag exists. The
/// app-id entry used by docks is still checked on every start because older
/// builds never created it; identical content is left untouched.
pub fn reapply_desktop_integration(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.is_debug {
        return;
    }

    let current_version = app.package_info().version.to_string();
    let flag_path = state.user_data.join("reapply-shortcuts.flag");
    let (auto_run, stored_version) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_bool("auto-run", false),
            persist.get_str("app-version", "").to_string(),
        )
    };
    let is_update_or_repair = stored_version != current_version || flag_path.exists();

    apply_auto_run(app, auto_run, is_update_or_repair);

    if is_update_or_repair {
        {
            let mut persist = state.persist.write().unwrap();
            let _ = persist.set("app-version", json!(current_version));
        }
        if flag_path.exists() {
            if let Err(err) = fs::remove_file(&flag_path) {
                tracing::warn!(%err, path = %flag_path.display(), "cannot remove shortcut repair flag");
            }
        }
        apply_desktop_shortcut(app);
    } else {
        ensure_app_desktop_entry(app);
    }
}
