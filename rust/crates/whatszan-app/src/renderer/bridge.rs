//! Electron-compatible renderer bridge, embedded in the Rust binary.
//!
//! WebKitGTK has no Rust DOM API, so this small script exposes the Tauri IPC
//! surface to the WhatsApp page. All application decisions remain in Rust.

pub const BRIDGE_JS: &str = r###"/*
 * Tauri replacement for src-web/preload.js.
 *
 * The renderer code (src-web/injected.js) was written against the Electron
 * preload API. This shim exposes the same surface on top of Tauri's
 * invoke/event bridge, so the WhatsApp DOM integration can be reused unchanged
 * while the application itself runs as a Rust binary.
 *
 * WRY (Tauri's webview layer) reserves `window.ipc` on Linux for its own
 * `postMessage` channel and defines it as a non-configurable, frozen property.
 * The Electron-compatible API therefore lives on `window.__wzIpc`; the
 * injected.js text is rewritten to that name when it is evaluated.
 */
(function () {
  if (window.__wzTauriBridgeInstalled) return;
  window.__wzTauriBridgeInstalled = true;

  // WebKitGTK exposes Safari's navigator identity (`vendor` is
  // "Apple Computer, Inc."), so WhatsApp Web offers the macOS app even when
  // the HTTP user agent is Chromium. The Electron build runs Chromium, which
  // reports `vendor = "Google Inc."` and a Linux platform. Expose the same
  // identity to page scripts; Rust supplies the configured user agent through
  // `window.__wzIdentity`.
  const wzIdentity = window.__wzIdentity || {};
  const wzUserAgent = wzIdentity.userAgent || navigator.userAgent;
  const wzPlatform = wzIdentity.platform || "Linux x86_64";
  const overrideNavigator = (name, value) => {
    try {
      Object.defineProperty(Object.getPrototypeOf(navigator), name, {
        get: () => value,
        configurable: true,
      });
    } catch (err) {
      console.warn(`[WhatsZan] cannot override navigator.${name}`, err);
    }
  };
  overrideNavigator("userAgent", wzUserAgent);
  overrideNavigator("appVersion", wzUserAgent.replace(/^Mozilla\//, ""));
  overrideNavigator("vendor", "Google Inc.");
  overrideNavigator("vendorSub", "");
  overrideNavigator("productSub", "20030107");
  overrideNavigator("platform", wzPlatform);
  overrideNavigator("appName", "Netscape");
  console.debug(
    "[WhatsZan] navigator identity",
    navigator.userAgent,
    navigator.vendor,
    navigator.platform,
  );

  // Electron's permission handlers make permission checks report "granted"
  // immediately. WebKitGTK only answers actual permission requests, so
  // WhatsApp Web shows its "message and call notifications are off" banner
  // even though every request is allowed. Mirror Electron's observable state
  // before any page script runs.
  const grantedPermissions = new Set([
    "notifications",
    "push",
    "microphone",
    "camera",
    "display-capture",
    "clipboard-read",
    "clipboard-write",
  ]);

  try {
    const notification = window.Notification;
    if (notification) {
      try {
        Object.defineProperty(notification, "permission", {
          get: () => "granted",
          configurable: true,
        });
      } catch (err) {
        console.warn("[WhatsZan] cannot override Notification.permission", err);
      }
      try {
        notification.requestPermission = () => Promise.resolve("granted");
      } catch (err) {
        console.warn(
          "[WhatsZan] cannot override Notification.requestPermission",
          err,
        );
      }
    }
  } catch (err) {
    console.warn("[WhatsZan] cannot install notification permission shim", err);
  }

  try {
    const permissions = navigator.permissions;
    if (permissions && typeof permissions.query === "function") {
      const originalQuery = permissions.query.bind(permissions);
      const query = (descriptor) => {
        if (descriptor && grantedPermissions.has(descriptor.name)) {
          const status = new EventTarget();
          status.state = "granted";
          return Promise.resolve(status);
        }
        return originalQuery(descriptor);
      };
      Object.defineProperty(Object.getPrototypeOf(permissions), "query", {
        value: query,
        configurable: true,
      });
    }
  } catch (err) {
    console.warn("[WhatsZan] cannot install permissions query shim", err);
  }

  const core = window.__TAURI__ && window.__TAURI__.core;
  const event = window.__TAURI__ && window.__TAURI__.event;

  if (!core) {
    console.error("[WhatsZan] Tauri global API is unavailable");
    return;
  }

  const invoke = (command, args) => core.invoke(command, args || {});
  const listen = (name, callback) => {
    if (!event) return Promise.resolve();
    return event.listen(name, (evt) => callback(evt.payload));
  };

  // Probe the Tauri IPC path once. This is the runtime check that remote-page
  // capabilities (capabilities/default.json) actually allow the bridge; if it
  // fails the error is visible in the WebKit console and in the Rust log.
  // WebKit also runs initialization scripts in the transient about:blank
  // document; invoking from there has an opaque origin and would only produce
  // a noisy "Origin header is not a valid URL" error.
  const isRealDocument = /^(https?|tauri):$/.test(location.protocol);
  if (isRealDocument) {
    core
      .invoke("log_to_main", { msg: `[bridge] IPC ready on ${location.host}` })
      .catch((err) => console.error("[WhatsZan] Tauri IPC probe failed", err));
  }

  // WebKit may report `PageLoadEvent::Finished` while its JavaScript context
  // still points at the previous about:blank document. The page itself is the
  // reliable signal: ask Rust to inject once the real document has loaded.
  // WhatsApp Web sometimes keeps a resource pending forever, so a timer backs
  // up the `load` event; Rust ignores duplicate calls.
  let pageReadySent = false;
  const notifyPageReady = () => {
    if (!isRealDocument || pageReadySent) return;
    pageReadySent = true;
    core.invoke("page_ready", { url: location.href }).then(
      () => console.log("[WhatsZan] page_ready sent", location.href),
      (err) => console.error("[WhatsZan] page_ready failed", err),
    );
  };
  if (document.readyState === "complete") {
    notifyPageReady();
  } else {
    window.addEventListener("load", notifyPageReady, { once: true });
    setTimeout(notifyPageReady, 15000);
  }

  window.__wzIpc = {
    debug: false,

    ping: () => invoke("ping"),
    notifyEv: (args) => invoke("notify_ev", { args }),
    open: (url) => invoke("open_url", { url }),
    stateGet: (name) => invoke("state_get", { name }),
    windowToggle: () => invoke("window_toggle"),
    escapePressed: () => invoke("escape_pressed"),
    togglePrivacyBlur: () => invoke("toggle_privacy_blur"),
    toggleAntiScreencast: () => invoke("toggle_anti_screencast"),
    logToMain: (msg) => invoke("log_to_main", { msg }),
    insertText: (text) => invoke("insert_text", { text }),
    updateRecentChats: (chats) => invoke("update_recent_chats", { chats }),
    updateUnread: (favicon) => invoke("update_unread", { favicon }),

    onJumpListAction: (callback) => listen("jump-list-action", callback),
    onSettingsSaved: (callback) => listen("settings-saved", callback),
    onNotifyClick: (callback) => listen("notify-click", callback),
    onNotifyReply: (callback) => listen("notify-reply", callback),
    onShareFiles: (callback) => listen("share-files-ready", callback),

    notifySend: (id, title, body, iconDataUrl) =>
      invoke("notify_send", { id, title, body, iconDataUrl }),

    getPendingShare: () => invoke("get_pending_share"),

    readFileBuffer: async (path) => {
      const result = await invoke("read_file_buffer", { path });
      if (!result) return null;
      const binary = atob(result.data);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i += 1) {
        bytes[i] = binary.charCodeAt(i);
      }
      return { name: result.name, type: result.type, buffer: bytes.buffer };
    },

    browseFolder: () => invoke("browse_folder"),
    getDefaultDownloadPaths: () => invoke("get_default_download_paths"),
    checkPathExists: (path) => invoke("check_path_exists", { path }),
  };

  window.screenPickerIpc = {
    selectSource: (sourceId) => invoke("screen_share_select", { sourceId }),
  };

  // GTK menu bars do not auto-hide like Electron's. Alt toggles the menu so
  // the same "press Alt to see the menu" workflow still works.
  window.addEventListener(
    "keydown",
    (evt) => {
      if (evt.key === "Alt" && !evt.ctrlKey && !evt.metaKey && !evt.shiftKey) {
        invoke("toggle_menu").catch(() => {});
      }
      if ((evt.ctrlKey || evt.metaKey) && evt.key === ",") {
        invoke("open_settings").catch(() => {});
        evt.preventDefault();
      }
    },
    true,
  );

  // The Electron main process emits page-favicon-updated. Tauri does not, so
  // watch the icon link and report changes to the Rust side.
  (function watchFavicon() {
    let last = null;
    const tick = () => {
      const link = document.querySelector('link[rel="icon"]');
      const href = link && link.href;
      if (href && href !== last) {
        last = href;
        window.__wzIpc.updateUnread(href).catch(() => {});
      }
    };
    tick();
    setInterval(tick, 2000);
  })();
})();
"###;
