//! Window creation and renderer integration.
//!
//! The WhatsApp DOM shim and privacy CSS are embedded in the Rust binary via
//! `crate::renderer`. Rust composes a feature snapshot (`window.__wzConfig`)
//! and injects the shim once the real page reports `page_ready`.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use serde_json::{json, Value};
use tauri::{
    webview::{NewWindowFeatures, NewWindowResponse},
    AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use whatszan_core::{
    convert_whatsapp_url, extract_real_url, is_web_whatsapp, is_whatsapp_internal, translations::t,
};

use crate::app_state::{AppState, ShareRequest};

const MAIN_LABEL: &str = "main";
const POPUP_LABEL_PREFIX: &str = "popup-";

const BRIDGE_JS: &str = crate::renderer::bridge::BRIDGE_JS;

static BOUNDS_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static POPUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The Rust-owned `frontend/debug.html` has no script of its own. WRY freezes
/// `window.ipc` on Linux, so the bridge lives on `window.__wzIpc`; install the
/// ping/notify/but1 handlers there after the debug page has loaded.
const DEBUG_PAGE_JS: &str = r#"(function () {
  if (window.__wzDebugPageInstalled) return;
  window.__wzDebugPageInstalled = true;

  const invoke = (command) => {
    const core = window.__TAURI__ && window.__TAURI__.core;
    if (!core) return Promise.reject(new Error("Tauri IPC unavailable"));
    return core.invoke(command);
  };

  const handlers = {
    ping: () => {
      invoke("ping").then(
        (reply) => console.log(`ping -> ${reply}`),
        (err) => console.error("ping failed", err),
      );
    },
    notify: () => {
      window.__wzIpc
        .notifySend(
          "debug-notification",
          "My notification",
          "Notification from the Rust debug page",
          null,
        )
        .catch((err) => console.error("notify failed", err));
    },
    but1: () => console.log("but1"),
  };

  for (const [id, handler] of Object.entries(handlers)) {
    const element = document.getElementById(id);
    if (element) element.onclick = handler;
  }

  addEventListener("keydown", (event) => console.debug("key", event));
})();"#;

fn default_url() -> Url {
    Url::parse("https://web.whatsapp.com/").expect("valid default URL")
}

fn configured_url(state: &AppState) -> Url {
    let config = state.config.read().unwrap();
    let raw = config.get_str("url", "https://web.whatsapp.com/");
    Url::parse(raw).unwrap_or_else(|_| default_url())
}

/// Identity shim installed before any page script runs. Popup windows need the
/// same script as the main window, otherwise WhatsApp sees WebKitGTK's Safari
/// user agent inside `window.open()` and offers the macOS application.
fn identity_init_script(state: &AppState) -> String {
    let user_agent = state.user_agent();
    format!(
        "window.__wzIdentity = {};\n{BRIDGE_JS}",
        json!({
            "userAgent": user_agent,
            "platform": "Linux x86_64",
        })
    )
}

fn window_background(state: &AppState) -> tauri::webview::Color {
    // WebKit paints white until the first frame arrives. The Electron window
    // starts with the WhatsApp palette, so match it here to avoid a white
    // flash while the (first) navigation is still in flight.
    if state.is_theme_dark() {
        tauri::webview::Color(17, 27, 33, 255)
    } else {
        tauri::webview::Color(248, 245, 243, 255)
    }
}

/// Build the main WhatsApp window.
pub fn create_main_window(app: &AppHandle, visible: bool) -> tauri::Result<WebviewWindow> {
    let state = app.state::<AppState>();
    let title = state.custom_app_name();
    let user_agent = state.user_agent();
    let is_debug = state.is_debug;
    let user_data = state.user_data.clone();
    let theme = if state.is_theme_dark() {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };

    let (bounds, maximized) = {
        let persist = state.persist.read().unwrap();
        (
            persist.get_or("window-bounds", json!({ "width": 1099, "height": 800 })),
            persist.get_bool("window-maximized", false),
        )
    };

    tracing::debug!(%user_agent, "creating WhatsApp window");

    let mut width = bounds
        .get("width")
        .and_then(Value::as_f64)
        .unwrap_or(1099.0);
    if is_debug {
        width += 1000.0;
    }
    let height = bounds
        .get("height")
        .and_then(Value::as_f64)
        .unwrap_or(800.0);
    let x = bounds.get("x").and_then(Value::as_f64);
    let y = bounds.get("y").and_then(Value::as_f64);

    let url = if is_debug {
        WebviewUrl::App("debug.html".into())
    } else {
        WebviewUrl::External(configured_url(&state))
    };

    // Expose the configured identity to the bridge before any page script
    // runs. WebKitGTK otherwise reports Safari's `navigator.vendor`, which
    // makes WhatsApp Web offer the macOS application.
    let init_script = identity_init_script(&state);
    let background = window_background(&state);

    let navigation_app = app.clone();
    let new_window_app = app.clone();
    let download_app = app.clone();

    let mut builder = WebviewWindowBuilder::new(app, MAIN_LABEL, url)
        .title(&title)
        .inner_size(width, height)
        .visible(visible)
        .resizable(true)
        .theme(Some(theme))
        .background_color(background)
        .user_agent(&user_agent)
        .data_directory(user_data.join("webkit"))
        .initialization_script(init_script)
        .on_navigation(move |url| on_navigation(&navigation_app, url))
        .on_new_window(move |url, features| on_new_window(&new_window_app, &url, features))
        .on_download(move |_webview, event| on_download(&download_app, event))
        .on_page_load(|window, payload| match payload.event() {
            tauri::webview::PageLoadEvent::Started => {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.set_main_page_loaded(false);
                    state.set_main_page_ready(false);
                }
            }
            tauri::webview::PageLoadEvent::Finished => {
                let url = payload.url().clone();
                tracing::debug!(%url, "main page finished loading");
                // Injection is driven by the renderer bridge's `page_ready`
                // call instead of this event: WebKit can emit Finished while
                // the JavaScript context still points at the previous
                // about:blank document, which made `eval` land in the wrong
                // frame. The bridge runs inside the real page, so its call is
                // authoritative.
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    let open_devtools = state.is_debug
                        || state
                            .config
                            .read()
                            .unwrap()
                            .get_bool("open-dev-tools", false);
                    if open_devtools && !window.is_devtools_open() {
                        window.open_devtools();
                    }
                }
            }
        });

    if let (Some(x), Some(y)) = (x, y) {
        builder = builder.position(x, y);
    }
    if maximized && visible {
        builder = builder.maximized(true);
    }

    let window = builder.build()?;
    state.set_main_window(window.clone());
    state.set_main_page_loaded(false);

    install_webkit_handlers(&window);
    attach_main_window_events(app.clone(), &window);
    set_window_icon(&window);
    set_dnd_mute(&window, state.is_dnd());
    crate::menu::apply_menu(app, &window);

    Ok(window)
}

/// Route a `window.open()` request.
///
/// Electron's `setWindowOpenHandler` allows WhatsApp's own popups and sends
/// every other URL to the desktop browser. `NewWindowResponse::Allow` would
/// produce a bare WRY webview without the Electron user agent, identity shim,
/// bridge, or permission handlers, so internal popups get a fully configured
/// Tauri window instead.
fn on_new_window(
    app: &AppHandle,
    url: &Url,
    features: NewWindowFeatures,
) -> NewWindowResponse<tauri::Wry> {
    if is_internal_popup(url) {
        match create_popup_window(app, url, features) {
            Ok(window) => return NewWindowResponse::Create { window },
            Err(err) => {
                tracing::warn!(%err, %url, "cannot create internal popup, blocking it");
                return NewWindowResponse::Deny;
            }
        }
    }

    tracing::info!(%url, "popup blocked, opening externally");
    handle_open_url(app, url.as_str());
    NewWindowResponse::Deny
}

/// URLs WhatsApp uses for `window.open()` windows, download tabs, and flows.
fn is_internal_popup(url: &Url) -> bool {
    matches!(url.scheme(), "about" | "blob" | "data") || is_whatsapp_internal(url.as_str())
}

fn create_popup_window(
    app: &AppHandle,
    url: &Url,
    features: NewWindowFeatures,
) -> tauri::Result<WebviewWindow> {
    let state = app.state::<AppState>();
    let title = state.custom_app_name();
    let user_agent = state.user_agent();
    let theme = if state.is_theme_dark() {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    let init_script = identity_init_script(&state);
    let background = window_background(&state);
    let muted = state.is_dnd();
    drop(state);

    let label = format!(
        "{POPUP_LABEL_PREFIX}{}",
        POPUP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let navigation_app = app.clone();
    let new_window_app = app.clone();
    let download_app = app.clone();

    // WebKit fills the returned webview with the requested `window.open()`
    // target. Start it at about:blank, matching WRY's own Allow path, so a
    // blob:/data: URL is not loaded twice or rejected by the WRY URL loader.
    let initial_url = Url::parse("about:blank").expect("valid about:blank URL");
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(initial_url))
        .title(&title)
        .inner_size(900.0, 700.0)
        .theme(Some(theme))
        .background_color(background)
        .user_agent(&user_agent)
        .initialization_script(init_script)
        .on_navigation(move |url| on_navigation(&navigation_app, url))
        .on_new_window(move |url, features| on_new_window(&new_window_app, &url, features))
        .on_download(move |_webview, event| on_download(&download_app, event))
        // Share the opener's WebKit process/context so cookies, storage, and
        // opener relationships behave like Electron child windows.
        .window_features(features);

    let window = builder.build()?;
    tracing::info!(%url, label, "created internal popup window");

    install_popup_webkit_handlers(&window);
    set_window_icon(&window);
    set_dnd_mute(&window, muted);
    Ok(window)
}

/// Close transient `window.open()` children when the main window goes away.
pub fn close_popup_windows(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(POPUP_LABEL_PREFIX) {
            let _ = window.close();
        }
    }
}

/// Open the native GTK settings window.
pub fn open_settings(app: &AppHandle) {
    crate::settings_ui::open(app);
}

pub fn toggle_main_window(app: &AppHandle) -> bool {
    let Some(window) = main_window(app) else {
        return false;
    };
    match window.is_visible() {
        Ok(true) => {
            hide_main_window(app);
            false
        }
        _ => {
            show_main_window(app);
            true
        }
    }
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide_main_window(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    close_chat(&window);
    close_popup_windows(app);
    let _ = window.hide();
}

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.try_state::<AppState>()?.main_window()
}

pub fn close_chat(window: &WebviewWindow) {
    let _ = window.eval("try { ewCloseChat(); } catch (e) {}");
}

/// Show the small in-page toast used by the privacy shortcuts.
pub fn show_toast(window: &WebviewWindow, message: &str) {
    let message = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
    let script = format!(
        r#"(function() {{
  let t = document.getElementById('wz-toast');
  if (!t) {{
    t = document.createElement('div');
    t.id = 'wz-toast';
    Object.assign(t.style, {{
      position: 'fixed', top: '24px', left: '50%', transform: 'translateX(-50%)',
      backgroundColor: 'rgba(0, 0, 0, 0.85)', color: '#fff', padding: '10px 20px',
      borderRadius: '20px', zIndex: '999999', fontSize: '14px', fontWeight: '500',
      transition: 'opacity 0.3s ease', opacity: '0', pointerEvents: 'none',
      boxShadow: '0 4px 12px rgba(0,0,0,0.3)'
    }});
    document.body.appendChild(t);
  }}
  t.innerText = {message};
  t.style.opacity = '1';
  if (t.timer) clearTimeout(t.timer);
  t.timer = setTimeout(() => t.style.opacity = '0', 2500);
}})();"#
    );
    let _ = window.eval(script);
}

/// Mute/unmute the WebKit view for Do Not Disturb.
pub fn set_dnd_mute(window: &WebviewWindow, muted: bool) {
    #[cfg(target_os = "linux")]
    {
        let _ = window.with_webview(move |webview| {
            use webkit2gtk::WebViewExt;
            webview.inner().set_is_muted(muted);
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, muted);
    }
}

/// Re-apply the user agent after settings are saved.
pub fn update_user_agent(window: &WebviewWindow, user_agent: &str) {
    #[cfg(target_os = "linux")]
    {
        let user_agent = user_agent.to_string();
        let _ = window.with_webview(move |webview| {
            use webkit2gtk::{SettingsExt, WebViewExt};
            if let Some(settings) = webview.inner().settings() {
                settings.set_user_agent(Some(&user_agent));
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, user_agent);
    }
}

/// Allow the permissions WhatsApp Web asks for (notifications, microphone,
/// camera), apply spellcheck preferences, and retry the main page when a load
/// fails (Electron's `did-fail-load` handler).
fn install_webkit_handlers(window: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        let app = window.app_handle().clone();
        let retry_window = window.clone();
        let _ = window.with_webview(move |webview| {
            use webkit2gtk::WebViewExt;
            let view = webview.inner();

            configure_webkit_view(&app, &view);

            if let (Some(state), Some(context)) = (app.try_state::<AppState>(), view.context()) {
                apply_preferred_languages_to_context(&context, &state);
                apply_spellcheck_to_context(&context, &state);
            }

            view.connect_load_changed(|view, event| {
                tracing::debug!(
                    ?event,
                    uri = ?view.uri(),
                    loading = view.is_loading(),
                    "webkit load changed"
                );
            });

            view.connect_load_failed(move |_view, _event, failing_uri, error| {
                tracing::warn!(uri = failing_uri, %error, "page load failed");
                // WebKit shows a stock error page when this signal is not
                // handled. Electron never does: it keeps the previous frame,
                // shows a notification, and retries. Returning true suppresses
                // that page for the main view as well as for popups.
                if !is_web_whatsapp(failing_uri) {
                    return true;
                }

                let app = retry_window.app_handle();
                let (interval, retry_url, app_name, failure_body) = app
                    .try_state::<AppState>()
                    .map(|state| {
                        (
                            state
                                .config
                                .read()
                                .unwrap()
                                .get_u64("retry-interval", 15_000),
                            configured_url(&state),
                            state.custom_app_name(),
                            t(&state.translations(), "load_failed", "Failed to load").to_string(),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            15_000,
                            default_url(),
                            "WhatsZan".into(),
                            "Failed to load".into(),
                        )
                    });

                // WebKit reports "Operation was cancelled" when a navigation is
                // superseded (for example the initial about:blank load). That
                // is not a user-visible failure, so don't notify for it and
                // retry almost immediately: the cancelled navigation usually
                // succeeds on the second attempt. Real failures keep the
                // configured backoff so an offline network is not hammered.
                let cancelled = error.message().to_ascii_lowercase().contains("cancel");
                if !cancelled {
                    crate::notifications::send(
                        app.clone(),
                        "load-failed".into(),
                        app_name,
                        failure_body,
                        None,
                    );
                }
                let delay = if cancelled { 600 } else { interval };

                let retry_window = retry_window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    let Some(state) = retry_window.app_handle().try_state::<AppState>() else {
                        return;
                    };
                    // A superseded navigation also reports "Operation was
                    // cancelled", and the replacement navigation may have
                    // succeeded. Only retry when the page never became ready.
                    if state.is_quitting() || state.is_main_page_ready() {
                        return;
                    }
                    tracing::info!("retrying WhatsApp Web after load failure");
                    // Navigate explicitly instead of `location.reload()`:
                    // when WebKit's JS context is still about:blank, reload
                    // would reload about:blank rather than WhatsApp Web.
                    let _ = retry_window.navigate(retry_url);
                });
                true
            });
        });
    }
}

/// Configure a WebKit view the way Electron configures its renderer:
/// media streams/WebRTC enabled, JavaScript popups allowed, and every
/// permission request (notifications, microphone, camera, display capture,
/// clipboard, ...) granted automatically like `setPermissionRequestHandler`.
#[cfg(target_os = "linux")]
fn configure_webkit_view(app: &AppHandle, view: &webkit2gtk::WebView) {
    use webkit2gtk::{
        glib::{object::ObjectExt, Cast},
        NotificationExt, PermissionRequestExt, SettingsExt, UserMediaPermissionRequestExt,
        WebViewExt,
    };

    if let Some(settings) = view.settings() {
        settings.set_enable_media_stream(true);
        settings.set_enable_media_capabilities(true);
        settings.set_enable_webaudio(true);
        settings.set_enable_webrtc(true);
        settings.set_javascript_can_access_clipboard(true);
        settings.set_javascript_can_open_windows_automatically(true);
        if tracing::enabled!(tracing::Level::DEBUG) {
            settings.set_enable_write_console_messages_to_stdout(true);
        }
    }

    view.connect_permission_request(|_view, request| {
        let kind = request.type_().name();
        if let Some(media) = request.downcast_ref::<webkit2gtk::UserMediaPermissionRequest>() {
            tracing::debug!(
                %kind,
                audio = media.is_for_audio_device(),
                video = media.is_for_video_device(),
                "allowing webkit media permission request"
            );
        } else {
            tracing::debug!(%kind, "allowing webkit permission request");
        }
        request.allow();
        true
    });

    // Page-level `new Notification()` calls are routed through the injected
    // bridge, but service-worker notifications bypass page JavaScript. Hand
    // those to the same XDG notification backend instead of letting WebKit's
    // default implementation drop or duplicate them.
    let notification_app = app.clone();
    view.connect_show_notification(move |_view, notification| {
        let state = notification_app.state::<AppState>();
        if state.is_dnd() {
            tracing::debug!("[DND] webkit notification blocked");
            return true;
        }

        let title = notification
            .title()
            .map(|title| title.to_string())
            .unwrap_or_else(|| state.custom_app_name());
        let body = notification
            .body()
            .map(|body| body.to_string())
            .unwrap_or_default();
        let id = format!("webkit-{}", notification.id());
        tracing::debug!(%id, %title, "routing webkit notification");
        crate::notifications::send(notification_app.clone(), id, title, body, None);
        true
    });
}

/// Popups only need the shared media/permission configuration. They must not
/// install the main window's load-failure retry, which navigates to WhatsApp.
fn install_popup_webkit_handlers(window: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        let app = window.app_handle().clone();
        let _ = window.with_webview(move |webview| {
            let view = webview.inner();
            configure_webkit_view(&app, &view);
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
    }
}

/// Apply the `spellcheck` / `spellcheck-languages` settings to the shared
/// WebKit context. `settings.html` can change these at runtime.
pub fn apply_spellcheck(window: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        let app = window.app_handle().clone();
        let _ = window.with_webview(move |webview| {
            use webkit2gtk::WebViewExt;
            if let (Some(state), Some(context)) =
                (app.try_state::<AppState>(), webview.inner().context())
            {
                apply_preferred_languages_to_context(&context, &state);
                apply_spellcheck_to_context(&context, &state);
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
    }
}

#[cfg(target_os = "linux")]
fn apply_preferred_languages_to_context(context: &webkit2gtk::WebContext, state: &AppState) {
    use webkit2gtk::WebContextExt;

    let languages = state.preferred_languages();
    if !languages.is_empty() {
        let languages: Vec<&str> = languages.iter().map(String::as_str).collect();
        context.set_preferred_languages(&languages);
    }
}

#[cfg(target_os = "linux")]
fn apply_spellcheck_to_context(context: &webkit2gtk::WebContext, state: &AppState) {
    use webkit2gtk::WebContextExt;

    let enabled = state.config.read().unwrap().get_bool("spellcheck", true);
    let languages = state.spellcheck_languages();

    context.set_spell_checking_enabled(enabled);
    if !languages.is_empty() {
        let languages: Vec<&str> = languages.iter().map(String::as_str).collect();
        context.set_spell_checking_languages(&languages);
    }
}

fn attach_main_window_events(app: AppHandle, window: &WebviewWindow) {
    window.on_window_event(move |event| {
        let Some(window) = main_window(&app) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested { api, .. } => {
                save_bounds(&window);
                let state = app.state::<AppState>();
                let quit_on_close = state
                    .config
                    .read()
                    .unwrap()
                    .get_bool("quit-on-close", false);
                if state.is_quitting() || quit_on_close {
                    state.set_quitting(true);
                    close_popup_windows(&app);
                    app.exit(0);
                } else {
                    api.prevent_close();
                    close_chat(&window);
                    close_popup_windows(&app);
                    let _ = window.hide();
                }
            }
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                schedule_bounds_save(app.clone());
            }
            _ => {}
        }
    });
}

fn schedule_bounds_save(app: AppHandle) {
    let sequence = BOUNDS_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(900)).await;
        if BOUNDS_SEQUENCE.load(Ordering::Relaxed) != sequence {
            return;
        }
        if let Some(window) = main_window(&app) {
            save_bounds(&window);
        }
    });
}

pub fn save_bounds(window: &WebviewWindow) {
    let app = window.app_handle();
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.is_debug {
        return;
    }

    let maximized = window.is_maximized().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    let scale = window.scale_factor().unwrap_or(1.0);
    let size = window.inner_size().ok();
    let position = window.outer_position().ok();

    let mut persist = state.persist.write().unwrap();
    let _ = persist.set("window-maximized", json!(maximized));
    if !maximized && !minimized {
        if let (Some(size), Some(position)) = (size, position) {
            // Persist logical coordinates, matching Electron's BrowserWindow
            // bounds and the values accepted by `position` / `inner_size`.
            let _ = persist.set(
                "window-bounds",
                json!({
                    "x": (position.x as f64 / scale).round() as i64,
                    "y": (position.y as f64 / scale).round() as i64,
                    "width": (size.width as f64 / scale).round() as u32,
                    "height": (size.height as f64 / scale).round() as u32,
                }),
            );
        }
    }
}

fn on_navigation(app: &AppHandle, url: &Url) -> bool {
    match url.scheme() {
        "about" | "blob" | "data" | "tauri" => {
            tracing::debug!(%url, "navigation allowed (internal scheme)");
            return true;
        }
        _ => {}
    }

    // WhatsApp loads its internal flows and static assets from
    // `*.whatsapp.net` (for example the hidden WAFlows cache-cleaner iframe).
    // Those are not external links and must stay inside the webview.
    if is_whatsapp_internal(url.as_str()) {
        tracing::debug!(%url, "navigation allowed (WhatsApp internal)");
        return true;
    }

    tracing::info!(%url, "navigation blocked, opening externally");
    handle_open_url(app, url.as_str());
    false
}

fn on_download(app: &AppHandle, event: tauri::webview::DownloadEvent<'_>) -> bool {
    match event {
        tauri::webview::DownloadEvent::Requested { destination, .. } => {
            let state = app.state::<AppState>();
            let smart = state
                .config
                .read()
                .unwrap()
                .get_bool("smart-download", true);
            if !smart {
                return true;
            }
            let filename = destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download");
            let extension = destination
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            let target_dir = download_directory(&state, &extension);
            if let Some(dir) = target_dir {
                let _ = fs::create_dir_all(&dir);
                let mut target = unique_destination(&dir, filename);
                if target.parent().is_none() {
                    target = dir.join(filename);
                }
                *destination = target;
            }
            true
        }
        tauri::webview::DownloadEvent::Finished { path, success, .. } => {
            tracing::debug!(?path, success, "download finished");
            true
        }
        _ => true,
    }
}

fn download_directory(state: &AppState, extension: &str) -> Option<PathBuf> {
    let config = state.config.read().unwrap();
    let configured = match extension {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" => {
            config.get_str("download-path-video", "").to_string()
        }
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" => {
            config.get_str("download-path-image", "").to_string()
        }
        "mp3" | "wav" | "ogg" | "m4a" | "flac" => {
            config.get_str("download-path-audio", "").to_string()
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "zip" | "rar" | "7z" => {
            config.get_str("download-path-document", "").to_string()
        }
        _ => String::new(),
    };

    if !configured.is_empty() {
        return Some(PathBuf::from(configured));
    }

    match extension {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" => dirs::video_dir(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" => dirs::picture_dir(),
        "mp3" | "wav" | "ogg" | "m4a" | "flac" => dirs::audio_dir(),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "zip" | "rar" | "7z" => {
            dirs::document_dir()
        }
        _ => dirs::download_dir(),
    }
}

fn unique_destination(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

    for counter in 1..10_000 {
        let name = if extension.is_empty() {
            format!("{stem} ({counter})")
        } else {
            format!("{stem} ({counter}).{extension}")
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(filename)
}

pub(crate) fn inject_page_scripts(window: WebviewWindow) {
    let app = window.app_handle().clone();
    let state = app.state::<AppState>();
    let app_name = state.custom_app_name();
    let window_url = window
        .url()
        .map(|url| url.to_string())
        .unwrap_or_else(|_| "<unknown>".into());
    tracing::info!(app = %app_name, %window_url, "injecting WhatsApp integration");

    if tracing::enabled!(tracing::Level::DEBUG) {
        let _ = window.with_webview(|webview| {
            use webkit2gtk::WebViewExt;
            tracing::info!(uri = ?webview.inner().uri(), "webview URI at injection");
        });
        let _ = window.eval(
            r#"try {
  console.log("[whatszan-diag]", JSON.stringify({
    tauri: typeof window.__TAURI__,
    core: typeof (window.__TAURI__ && window.__TAURI__.core),
    internals: typeof window.__TAURI_INTERNALS__,
    invoke: typeof (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke),
    ipc: typeof window.ipc,
    ipcKeys: window.ipc ? Object.keys(window.ipc) : null,
    bridge: !!window.__wzTauriBridgeInstalled,
    href: location.href,
    readyState: document.readyState,
    title: document.title,
    top: window === window.top,
    frames: window.frames.length,
    host: location.host
  }));
} catch (e) { console.error("[whatszan-diag] failed", e); }"#,
        );
    }

    let config = state.renderer_config();
    let script = crate::renderer::injected_script(&app_name, &config);
    if let Err(err) = window.eval(script) {
        tracing::warn!(%err, "cannot inject WhatsApp integration");
    }

    if state.is_debug
        && (window_url.starts_with("tauri://") || window_url.starts_with("http://tauri.localhost"))
    {
        if let Err(err) = window.eval(DEBUG_PAGE_JS) {
            tracing::warn!(%err, "cannot install debug page handlers");
        }
    }

    if let Err(err) = window.eval(crate::renderer::privacy_style_script()) {
        tracing::warn!(%err, "cannot inject privacy stylesheet");
    }

    apply_blur_state(&app, &window);
    apply_user_scripts(&app, &window);
    apply_user_css(&app, &window);
}

/// Push the latest Rust-owned feature snapshot into a live renderer.
pub fn apply_renderer_config(window: &WebviewWindow, state: &AppState) {
    let script = crate::renderer::config_script(&state.renderer_config());
    if let Err(err) = window.eval(script) {
        tracing::debug!(%err, "cannot refresh renderer config");
    }
}

/// Apply privacy-blur classes and CSS variables. This is the same logic as
/// `updateBlurState` in `src/main.mjs`.
pub fn apply_blur_state(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<AppState>();
    let (
        incognito_no_read,
        incognito_no_typing,
        incognito_anti_delete,
        blur_contacts,
        blur_pp,
        blur_messages,
        blur_media,
        blur_input,
        blur_specific,
        blur_amount,
        unblur_delay,
    ) = {
        let config = state.config.read().unwrap();
        (
            config.get_bool("incognito-noread", false),
            config.get_bool("incognito-notyping", false),
            config.get_bool("incognito-antidelete", false),
            config.get_bool("blur-contacts", true),
            config.get_bool("blur-pp", true),
            config.get_bool("blur-messages", true),
            config.get_bool("blur-media", true),
            config.get_bool("blur-input", true),
            config.get_str("blur-specific", "").to_string(),
            config.get_f64("blur-amount", 4.0),
            config.get_f64("unblur-delay", 0.0),
        )
    };

    let master = state
        .persist
        .read()
        .unwrap()
        .get_bool("privacy-blur", false);

    let specific = serde_json::to_string(&blur_specific).unwrap_or_else(|_| "\"\"".into());
    let script = format!(
        r#"(function() {{
  window.wzIncognito = {{
    noRead: {incognito_no_read},
    noTyping: {incognito_no_typing},
    antiDelete: {incognito_anti_delete}
  }};
  const root = document.documentElement;
  root.style.setProperty('--wz-blur-amount', '{blur_amount}px');
  root.style.setProperty('--wz-blur-media', '{media_amount}px');
  root.style.setProperty('--wz-unblur-delay', '{unblur_delay}s');
  const body = document.body;
  if (!body) return;
  body.classList.remove('blur-contacts', 'blur-pp', 'blur-messages', 'blur-media', 'blur-input', 'privacy-blur');
  const oldStyle = document.getElementById('whatszan-blur-specific');
  if (oldStyle) oldStyle.remove();

  if ({master}) {{
    if ({blur_pp}) body.classList.add('blur-pp');
    if ({blur_messages}) body.classList.add('blur-messages');
    if ({blur_media}) body.classList.add('blur-media');
    if ({blur_input}) body.classList.add('blur-input');

    const specific = {specific};
    const names = specific.split(',').map(n => n.trim()).filter(n => n);
    if (names.length > 0) {{
      let css = '';
      names.forEach(name => {{
        css += `[title="${{name}}"] {{ filter: blur(var(--wz-blur-amount, 4px)); opacity: 0.75; transform: translateZ(0); transition: filter 0.15s ease-out; }}\n`;
        css += `div[role="row"]:hover [title="${{name}}"], div[role="listitem"]:hover [title="${{name}}"], header:hover [title="${{name}}"], [title="${{name}}"]:hover {{ filter: blur(0px); opacity: 1; transition: filter 0.05s ease-in; }}\n`;
      }});
      const style = document.createElement('style');
      style.id = 'whatszan-blur-specific';
      style.textContent = css;
      (document.head || root).appendChild(style);
    }} else if ({blur_contacts}) {{
      body.classList.add('blur-contacts');
    }}
  }}
}})();"#,
        media_amount = blur_amount * 1.5,
    );

    if let Err(err) = window.eval(script) {
        tracing::debug!(%err, "cannot apply privacy blur");
    }
}

fn apply_user_scripts(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<AppState>();
    let scripts = {
        let config = state.config.read().unwrap();
        config.get_array("scripts")
    };
    if scripts.is_empty() {
        return;
    }

    let root = state.user_data.join("user-scripts");
    let root = fs::canonicalize(&root).unwrap_or(root);

    for entry in scripts {
        let Some(raw) = entry.as_str() else {
            continue;
        };
        let expanded = raw.replace("${userData}", &state.user_data.display().to_string());
        let path =
            fs::canonicalize(PathBuf::from(&expanded)).unwrap_or_else(|_| PathBuf::from(&expanded));
        if !path.starts_with(&root) {
            tracing::warn!(path = %path.display(), root = %root.display(), "user script outside user-scripts directory");
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("js") {
            tracing::warn!(path = %path.display(), "user script must end with .js");
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(script) => {
                tracing::info!(path = %path.display(), "loading user script");
                let _ = window.eval(script);
            }
            Err(err) => tracing::warn!(path = %path.display(), %err, "cannot read user script"),
        }
    }
}

fn apply_user_css(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<AppState>();
    let entries = {
        let config = state.config.read().unwrap();
        config.get_array("css")
    };

    for (index, entry) in entries.into_iter().enumerate() {
        let Some(raw) = entry.as_str() else {
            continue;
        };
        let raw = raw.to_string();
        let parsed = Url::parse(&raw).ok();

        match parsed {
            Some(url) if url.scheme() == "file" => {
                let path = file_url_to_path(&url);
                if let Some(path) = path {
                    if let Ok(css) = fs::read_to_string(path) {
                        inject_css(window, index, &css);
                    }
                }
            }
            Some(url) if url.scheme() == "https" => {
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    let css = tokio::task::spawn_blocking(move || {
                        ureq::get(url.as_str())
                            .timeout(Duration::from_secs(15))
                            .call()
                            .ok()
                            .and_then(|response| response.into_string().ok())
                    })
                    .await
                    .ok()
                    .flatten();
                    if let Some(css) = css {
                        inject_css(&window, index, &css);
                    }
                });
            }
            _ => inject_css(window, index, &raw),
        }
    }
}

fn file_url_to_path(url: &Url) -> Option<PathBuf> {
    if url.host_str() == Some("~") {
        let home = dirs::home_dir()?;
        return Some(home.join(url.path().trim_start_matches('/')));
    }
    url.to_file_path().ok()
}

fn inject_css(window: &WebviewWindow, index: usize, css: &str) {
    let css = serde_json::to_string(css).unwrap_or_else(|_| "\"\"".into());
    let _ = window.eval(format!(
        r#"(function() {{
  const id = 'whatszan-user-css-{index}';
  let style = document.getElementById(id);
  if (!style) {{
    style = document.createElement('style');
    style.id = id;
    (document.head || document.documentElement).appendChild(style);
  }}
  style.textContent = {css};
}})();"#
    ));
}

/// Route an `ipc.open()` / navigation URL.
pub fn handle_open_url(app: &AppHandle, url: &str) {
    let real = extract_real_url(url);
    if let Some(web_url) = convert_whatsapp_url(&real) {
        navigate_main(app, &web_url);
        return;
    }

    let Ok(parsed) = Url::parse(&real) else {
        return;
    };

    // Electron deliberately ignores links that already point at WhatsApp Web:
    // WhatsApp handles those inside the page itself. Only genuinely external
    // URLs are handed to the desktop.
    if parsed.host_str() == Some("web.whatsapp.com") {
        return;
    }

    if matches!(parsed.scheme(), "http" | "https" | "mailto" | "tel" | "sms") {
        // Open the original URL (not the unwrapped tracker target), matching
        // the Electron build's privacy behaviour.
        if let Err(err) = open::that(url) {
            tracing::warn!(%err, url, "cannot open external URL");
        }
    }
}

pub fn navigate_main(app: &AppHandle, url: &str) {
    let Some(window) = main_window(app) else {
        return;
    };
    show_main_window(app);

    let currently_whatsapp = window
        .url()
        .map(|current| is_web_whatsapp(current.as_str()))
        .unwrap_or(false);

    if currently_whatsapp {
        let url = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into());
        let script = format!(
            r#"(() => {{
  try {{
    const a = document.createElement('a');
    a.href = {url};
    document.body.appendChild(a);
    a.click();
    a.remove();
    return true;
  }} catch (e) {{
    return false;
  }}
}})();"#
        );
        if window.eval(script).is_ok() {
            return;
        }
    }

    if let Ok(parsed) = Url::parse(url) {
        let _ = window.navigate(parsed);
    }
}

/// Handle `whatsapp://` URLs and chat invites passed on the command line.
pub fn handle_whatsapp_protocol(app: &AppHandle, url: &str) {
    if let Some(web_url) = convert_whatsapp_url(url) {
        tracing::info!(url, %web_url, "loading WhatsApp URL");
        navigate_main(app, &web_url);
    }
}

/// Handle `--open-chat=` and `--action=` arguments.
pub fn handle_jump_list_args(app: &AppHandle, args: &[String]) -> bool {
    let Some(value) = args.iter().find_map(|arg| arg.strip_prefix("--open-chat=")) else {
        if let Some(action) = args.iter().find_map(|arg| arg.strip_prefix("--action=")) {
            let action = action.trim_matches('"').to_string();
            dispatch_jump(app, json!({ "type": "action", "name": action }));
            return true;
        }
        return false;
    };

    let chat = value.trim_matches('"').to_string();
    dispatch_jump(app, json!({ "type": "open-chat", "name": chat }));
    true
}

fn dispatch_jump(app: &AppHandle, payload: Value) {
    show_main_window(app);
    let state = app.state::<AppState>();
    if state.is_main_page_loaded() {
        if let Some(window) = state.main_window() {
            let _ = window.emit("jump-list-action", payload);
        }
    } else {
        *state.pending_jump.lock().unwrap() = Some(payload);
    }
}

pub(crate) fn emit_pending_events(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(window) = state.main_window() {
        if let Some(payload) = state.pending_jump.lock().unwrap().take() {
            let _ = window.emit("jump-list-action", payload);
        }
        if state.pending_share.lock().unwrap().is_some() {
            let _ = window.emit("share-files-ready", ());
        }
    }
}

/// Parse `--share-media` / `--share-document` arguments.
pub fn handle_share_args(app: &AppHandle, args: &[String]) -> bool {
    let is_media = args.iter().any(|arg| arg == "--share-media");
    let is_document = args.iter().any(|arg| arg == "--share-document");
    if !is_media && !is_document {
        return false;
    }

    let mut kind = if is_media { "media" } else { "document" }.to_string();
    let mut files = Vec::new();

    for arg in args {
        if arg.starts_with('-') || arg == "." {
            continue;
        }
        let path = PathBuf::from(arg);
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            if let Some(zip) = zip_directory(&path) {
                files.push(zip.display().to_string());
            }
        } else {
            files.push(path.display().to_string());
        }
    }

    if kind == "media"
        && files.iter().any(|file| {
            let ext = Path::new(file)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            !matches!(
                ext.as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "mp4" | "mov" | "avi"
            )
        })
    {
        kind = "document".into();
    }

    if files.is_empty() {
        return true;
    }

    let state = app.state::<AppState>();
    let translations = state.translations();
    let toast_text = t(
        &translations,
        "toast_open_chat",
        "WhatsZan: Silakan buka salah satu chat untuk mengirim file ini.",
    )
    .to_string();

    *state.pending_share.lock().unwrap() = Some(ShareRequest {
        kind,
        files,
        toast_text,
    });

    show_main_window(app);
    if state.is_main_page_loaded() {
        if let Some(window) = state.main_window() {
            let _ = window.emit("share-files-ready", ());
        }
    }
    true
}

fn zip_directory(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy();
    let destination = std::env::temp_dir().join(format!("WhatsZan_{name}.zip"));
    let status = std::process::Command::new("zip")
        .arg("-r")
        .arg("-q")
        .arg(&destination)
        .arg(path)
        .status();
    match status {
        Ok(status) if status.success() => Some(destination),
        _ => {
            tracing::warn!(path = %path.display(), "cannot zip shared directory");
            None
        }
    }
}

/// Set the window/taskbar icon from the persisted branding choice.
pub fn set_window_icon(window: &WebviewWindow) {
    let app = window.app_handle();
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let custom = state
        .persist
        .read()
        .unwrap()
        .get_str("custom_tray_app", "")
        .to_string();
    let icon = if custom.is_empty() {
        load_tauri_icon_bytes(crate::assets::WINDOW_ICON_PNG)
    } else {
        load_tauri_icon(Path::new(&custom))
    };
    if let Some(icon) = icon {
        let _ = window.set_icon(icon);
    }
}

pub fn load_tauri_icon(path: &Path) -> Option<tauri::image::Image<'static>> {
    let image = image::open(path).ok()?.to_rgba8();
    Some(tauri_image(image))
}

pub fn load_tauri_icon_bytes(bytes: &[u8]) -> Option<tauri::image::Image<'static>> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(tauri_image(image))
}

fn tauri_image(image: image::RgbaImage) -> tauri::image::Image<'static> {
    let (width, height) = image.dimensions();
    tauri::image::Image::new_owned(image.into_raw(), width, height)
}
