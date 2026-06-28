import { app, BrowserWindow, session, Menu, MenuItem, Tray, ipcMain, shell, Notification, dialog, nativeTheme, desktopCapturer, nativeImage } from "electron";
import { readFileSync, existsSync, unlinkSync } from "node:fs";
import path from "node:path";
import {
  addAboutMenuItem,
  isDebug,
  toggleVisibility,
  windowShow,
  getUrl,
  loadUrl,
  replaceVariables,
  getIcon,
  getUserIcon,
  loadTranslations,
  getUnreadCountFromFavicon,
  convertWhatsAppUrl,
  urlScheme,
  getBadgedTrayIcon,
  monitorSystemTheme,
} from "./util.mjs";
import contextMenu from "electron-context-menu";
import { debounce } from "lodash-es";
import { consola, LogLevels } from "consola";

import pkg from "../package.json" with { type: "json" };
import { JsonConfig } from "./json-config.mjs";
import { defaultKeys } from "./keys.mjs";
import { Dbus } from "./dbus.mjs";
import { applyAutoRun, applyDesktopShortcut } from "./shortcuts.mjs";
import { setupAppMenu } from "./menu.mjs";
import { setupTray } from "./tray.mjs";


const defaultUserAgent =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

function handleWhatsAppProtocol(window, url) {
  const webUrl = convertWhatsAppUrl(url);
  if (webUrl) {
    consola.info("Loading WhatsApp URL:", webUrl);
    windowShow(window);

    const currentUrl = window.webContents.getURL();
    const isLoaded = currentUrl.includes("web.whatsapp.com");

    if (isLoaded) {
      window.webContents.executeJavaScript(`
        try {
          const a = document.createElement('a');
          a.href = ${JSON.stringify(webUrl)};
          document.body.appendChild(a);
          a.click();
          a.remove();
          true;
        } catch(e) {
          false;
        }
      `).then(success => {
        if (!success) {
          window.webContents.loadURL(webUrl);
        }
      }).catch(err => {
        window.webContents.loadURL(webUrl);
      });
    } else {
      window.webContents.loadURL(webUrl);
    }
  }
}

let pendingShare = null;

function handleShareArgs(window, commandLine) {
  const isMedia = commandLine.includes("--share-media");
  const isDoc = commandLine.includes("--share-document");
  
  if (!isMedia && !isDoc) return false;
  
  (async () => {
    let type = isMedia ? "media" : "document";
    const files = [];
    let hasNonMedia = false;
    
    const { statSync } = await import('node:fs');
    const { exec } = await import('node:child_process');
    
    for (const arg of commandLine) {
      if (!arg.startsWith("--") && !arg.endsWith(".exe") && arg !== ".") {
        try {
          if (existsSync(arg)) {
            if (statSync(arg).isDirectory()) {
              const zipPath = path.join(app.getPath("temp"), `WhatsZan_${path.basename(arg)}.zip`);
              if (process.platform === 'win32') {
                if (window) windowShow(window);
                await new Promise((resolve) => {
                  exec(`powershell.exe -NoProfile -Command "Compress-Archive -Path '${arg}\\*' -DestinationPath '${zipPath}' -Force"`, resolve);
                });
                files.push(zipPath);
                hasNonMedia = true;
              }
            } else {
              files.push(arg);
            }
          }
        } catch(e) {}
      }
    }
    
    if (type === "media") {
      for (const f of files) {
        const ext = path.extname(f).toLowerCase();
        if (!['.jpg', '.jpeg', '.png', '.gif', '.webp', '.mp4', '.mov', '.avi'].includes(ext)) {
          hasNonMedia = true;
          break;
        }
      }
    }
    
    if (hasNonMedia && type === "media") {
      type = "document";
    }
    
    if (files.length > 0) {
      const config = new JsonConfig(path.join(app.getPath("userData"), "config.json"));
      const appLang = config.get("app-language", "auto");
      const lang = appLang !== "auto" ? appLang : app.getLocale();
      const { loadTranslations } = await import("./util.mjs");
      const translations = loadTranslations(lang);
      
      pendingShare = { 
        type, 
        files, 
        toastText: translations.toast_open_chat || "WhatsZan: Silakan buka salah satu chat untuk mengirim file ini."
      };
      if (window && window.webContents) {
        windowShow(window);
        window.webContents.send("share-files-ready");
      }
    }
  })();
  
  return true;
}

function main() {
  const config = new JsonConfig(path.join(app.getPath("userData"), "config.json"));

  // Configure log level: CLI option takes priority over config option
  const cliLogLevel = process.argv.find((arg) => arg.startsWith("--log-level="))?.split("=")[1];
  const configLogLevel = config.get("log-level", "info");
  const logLevelStr = cliLogLevel || configLogLevel;
  consola.level = LogLevels[logLevelStr] ?? 3;

  consola.info("config file", config.file);

  const persistStateFileName = path.join(app.getPath("userData"), "persistent-state.json");
  const persistState = new JsonConfig(persistStateFileName);
  consola.info("state file", persistState.file);

  const isHiddenStartup = process.argv.includes("--hide");

  const updateAppUserModel = (appName, iconPath) => {
    if (process.platform !== 'win32') return;
    const appId = 'org.uznbt.whatszan'; // Constant AUMID to prevent taskbar duplication
    app.setAppUserModelId(appId);
    try {
      app.setAppDetails({
        appId: appId,
        appIconPath: iconPath || process.execPath,
        appIconIndex: 0,
        relaunchCommand: process.execPath,
        relaunchDisplayName: appName
      });
    } catch (e) {}
  };

  const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
  updateAppUserModel(customAppName, persistState.get("custom_tray_app"));

  const state = {
    notifPrefix: config.get("notification-prefix", ""),
    showAtStartup: isDebug || !isHiddenStartup,
    escToggle: config.get("esc-toggle-window", false),
    get textReplacements() {
      return config.get("text-replacements", {});
    },
    get windowBounds() {
      const bounds = persistState.get("window-bounds", { width: 1099, height: 800 });
      if (isDebug) {
        bounds.width += 1000;
      }
      return bounds;
    },
    get keys() {
      return { ...defaultKeys, ...config.get("keys", {}) };
    },
    get userAgent() {
      return config.get("user-agent", defaultUserAgent);
    },
    get iconsDir() {
      return config.get("icons-directory", path.join(app.getPath("userData"), "user-icons"));
    },
  };

  if (config.get("quit-on-close", false)) {
    state.showAtStartup = true;
  }

  // Helper untuk memastikan shortcut startup sesuai dengan kondisi terakhir
  // (Logic moved to src/shortcuts.mjs)
  // Selalu pastikan setting auto-run aktif saat startup (misal pasca-update)
  if (!isDebug) {
    applyAutoRun(persistState.get("auto-run", false));
    
    // Check if app was just updated or repaired to re-apply custom shortcuts
    const currentVersion = app.getVersion();
    const flagPath = path.join(app.getPath('userData'), 'reapply-shortcuts.flag');
    if (persistState.get("app-version") !== currentVersion || existsSync(flagPath)) {
      persistState.set("app-version", currentVersion);
      if (existsSync(flagPath)) {
        try { unlinkSync(flagPath); } catch (e) {}
      }
      const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
      applyDesktopShortcut(persistState.get("desktop-shortcut", true), customAppName, customAppName, config, persistState);
    }
  }

  // Auto-load saved extensions on startup
  const savedExtensions = persistState.get("extensions", []);
  if (savedExtensions.length > 0) {
    import('electron-extension-installer').then(({ installExtension }) => {
      installExtension(savedExtensions, {
        loadExtensionOptions: { allowFileAccess: true }
      }).then((names) => {
        consola.info('Loaded extensions:', names);
      }).catch(err => {
        consola.error('Failed to load extensions:', err);
      });
    }).catch(e => consola.error('Could not load extension installer', e));
  }

  const createWindow = async () => {
    // Create the browser window.
    const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
    const customAppIcon = persistState.get("custom_tray_app");
    const mainWindow = new BrowserWindow({
      title: customAppName,
      icon: customAppIcon || path.join(import.meta.dirname, '..', 'build', 'icon.ico'),
      webPreferences: {
        preload: path.join(import.meta.dirname, "..", "src-web", "preload.js"),
        spellcheck: config.get("spellcheck", true),
      },
      show: state.showAtStartup,
      autoHideMenuBar: config.get("menu-bar-auto-hide", true),
      ...state.windowBounds,
    });

    mainWindow.setContentProtection(persistState.get("anti-screencast", false));

    if (!config.get("menu-bar", true)) {
      mainWindow.removeMenu();
    }

    if (persistState.get("window-maximized", false)) {
      mainWindow.maximize();
    }

    if (isDebug || config.get("open-dev-tools", false)) {
      mainWindow.webContents.openDevTools();
    }


    if (config.get("register-url-scheme", true) && !app.isDefaultProtocolClient(urlScheme)) {
      consola.info(`Registering as default protocol client for ${urlScheme}://`);
      app.setAsDefaultProtocolClient(urlScheme);
    }

    // Sets the spellchecker langs
    const preferredLangs = app.getPreferredSystemLanguages();
    if (preferredLangs.length == 0) {
      preferredLangs.push("en-US");
    }
    consola.debug("preferredLangs", preferredLangs);

    const spellLangs = config.get("spellcheck-languages", preferredLangs);
    consola.debug("spellLangs", spellLangs);

    try {
      session.defaultSession.setSpellCheckerLanguages(spellLangs);
    } catch (err) {
      consola.error("setSpellCheckerLanguages", err);
    }

    const appLang = config.get("app-language", "auto");
    const lang = appLang !== "auto" ? appLang : preferredLangs[0];
    const translations = loadTranslations(lang);

    contextMenu({
      showSelectAll: false,
      showSaveImageAs: true,
      showSaveVideoAs: true,
      showSearchWithGoogle: false,
      showInspectElement: isDebug,
      labels: translations,
    });

    session.defaultSession.webRequest.onBeforeSendHeaders((details, callback) => {
      details.requestHeaders["User-Agent"] = state.userAgent;
      if (appLang !== "auto") {
        details.requestHeaders['Accept-Language'] = `${appLang},en;q=0.9`;
      }
      callback({ cancel: false, requestHeaders: details.requestHeaders });
    });

    session.defaultSession.setPermissionRequestHandler((webContents, permission, callback) => {
      callback(true);
    });

    session.defaultSession.setPermissionCheckHandler((webContents, permission) => {
      return true;
    });

    // ── Screen Share Handler ─────────────────────────────────────────────────
    // ── Screen Share Handler ─────────────────────────────────────────────────
    // Menangani getDisplayMedia() dari WhatsApp Web saat VC screen share.
    // Menampilkan custom picker window dengan thumbnail layar/jendela.
    session.defaultSession.setDisplayMediaRequestHandler(async (request, callback) => {
      let callbackCalled = false;
      const safeCallback = (result) => {
        if (callbackCalled) return;
        callbackCalled = true;
        try {
          callback(result);
        } catch (e) {
          // Electron throws jika cancel (video requested tapi tidak ada stream)
          consola.debug('Screen share callback suppressed:', e.message);
        }
      };

      try {
        // Ambil semua sumber layar dan window dengan thumbnail
        const sources = await desktopCapturer.getSources({
          types: ['screen', 'window'],
          thumbnailSize: { width: 300, height: 200 },
          fetchWindowIcons: true,
        });

        if (sources.length === 0) {
          consola.warn('Screen share: no sources found');
          safeCallback({});
          return;
        }

        // Buka picker window
        const pickerWin = new BrowserWindow({
          width: 680,
          height: 500,
          resizable: false,
          minimizable: false,
          maximizable: false,
          alwaysOnTop: true,
          title: translations.share_screen || 'Bagikan Layar',
          autoHideMenuBar: true,
          webPreferences: {
            nodeIntegration: false,
            contextIsolation: true,
            preload: path.join(import.meta.dirname, '..', 'src-web', 'preload.js'),
          },
        });

        // Deteksi tema dari halaman WA yang sedang berjalan (karena bisa beda dengan tema OS)
        let isWaDark = nativeTheme.shouldUseDarkColors;
        try {
          isWaDark = await mainWindow.webContents.executeJavaScript(`
            document.documentElement.classList.contains('dark') || document.body.classList.contains('dark')
          `);
        } catch (e) {
          consola.debug('Gagal deteksi tema WA, menggunakan tema OS');
        }

        const queryStr = `theme=${isWaDark ? 'dark' : 'light'}&title=${encodeURIComponent(translations.share_screen || 'Bagikan Layar')}&desc=${encodeURIComponent(translations.select_screen || 'Pilih layar atau jendela yang ingin dibagikan')}&btn=${encodeURIComponent(translations.share || 'Bagikan')}&tCancel=${encodeURIComponent(translations.cancel || 'Batal')}&tScreen=${encodeURIComponent(translations.tab_screens || 'Layar')}&tWin=${encodeURIComponent(translations.tab_windows || 'Jendela')}&tLoading=${encodeURIComponent(translations.screen_picker_loading || 'Memuat sumber...')}&tEmpty=${encodeURIComponent(translations.screen_picker_empty || 'Tidak ada {type} yang tersedia')}&tTypeS=${encodeURIComponent(translations.screen_picker_screens || 'layar')}&tTypeW=${encodeURIComponent(translations.screen_picker_windows || 'jendela')}`;
        pickerWin.loadFile(path.join(import.meta.dirname, '..', 'static', 'screen-picker.html'), { search: queryStr });
        pickerWin.webContents.on('did-finish-load', () => {
          const sourcesData = sources.map(s => ({
            id: s.id,
            name: s.name,
            thumbnail: s.thumbnail?.toDataURL() || null,
          }));
          pickerWin.webContents.executeJavaScript(
            `window.electronAPI?.setSources(${JSON.stringify(sourcesData)}, ${isWaDark})`
          );
        });



        // Terima pilihan dari picker via IPC
        ipcMain.handleOnce('screen-share-select', (ev, sourceId) => {
          pickerWin.close();
          if (sourceId) {
            const selected = sources.find(s => s.id === sourceId);
            if (selected) {
              consola.info('Screen share: user selected', selected.name);
              safeCallback({ video: selected });
            } else {
              safeCallback({});
            }
          } else {
            consola.info('Screen share: user cancelled');
            safeCallback({});
          }
        });

        // Jika picker ditutup paksa (X button)
        pickerWin.on('closed', () => {
          safeCallback({});
        });

        // Tutup otomatis saat kehilangan fokus (klik di luar)
        pickerWin.on('blur', () => {
          if (!pickerWin.isDestroyed()) pickerWin.close();
        });

      } catch (err) {
        consola.error('Screen share handler error', err);
        safeCallback({});
      }
    });
    // ────────────────────────────────────────────────────────────────────────


    const saveBounds = () => {
      if (!isDebug && !mainWindow.isDestroyed()) {
        const isMaximized = mainWindow.isMaximized();
        persistState.set("window-maximized", isMaximized);
        if (!isMaximized && !mainWindow.isMinimized()) {
           persistState.set("window-bounds", mainWindow.getBounds());
        } else if (isMaximized || mainWindow.isMinimized()) {
           persistState.set("window-bounds", mainWindow.getNormalBounds());
        }
      }
    };
    const closeChat = () => {
      // or use https://www.electronjs.org/docs/latest/tutorial/ipc#pattern-3-main-to-renderer
      mainWindow.webContents.executeJavaScript("ewCloseChat()");
    };

    const debounced = debounce(saveBounds, 1000);
    mainWindow.on("move", debounced);
    mainWindow.on("resize", debounced);
    mainWindow.on("close", saveBounds);
    mainWindow.on("hide", () => {
      closeChat();
    });

    mainWindow.on("close", function (event) {
      if (config.get("quit-on-close", false)) {
        app.quit();
      } else {
        consola.debug(`close ${app.isQuiting}`);
        if (!app.isQuiting) {
          event.preventDefault();
          closeChat();
          mainWindow.hide();
        }
      }
    });

    app.on("before-quit", function () {
      consola.debug("before-quit");
      app.isQuiting = true;
    });

    app.whenReady().then(() => {
      if (config.parseError) {
        const res = dialog.showMessageBoxSync({
          type: "error",
          buttons: [
            translations?.config_continue ?? "Continue with default config",
            translations?.config_quit ?? "Quit",
          ],
          title: translations?.config_title ?? "Configuration file",
          message: config.file + ":\n\n" + config.parseError,
        });
        if (res === 1) {
          app.quit();
        }
      }
      
      monitorSystemTheme(nativeTheme);

      let dbus;
      if (config.get("dbus", true)) {
        dbus = new Dbus(mainWindow);
      }
      if (isDebug) {
        ipcMain.handle("ping", () => "pong");
      }
      ipcMain.handle("notifyEv", (ev, argsJson) => {
        windowShow(mainWindow);
      });

      ipcMain.handle("notifySend", (ev, id, title, body, iconDataUrl) => {
        let iconOpts = {};
        if (iconDataUrl) {
          try {
            iconOpts = { icon: nativeImage.createFromDataURL(iconDataUrl) };
          } catch(e) { }
        }

        const notif = new Notification({
          title: title,
          body: body,
          ...iconOpts,
          hasReply: true,
          replyPlaceholder: 'Balas pesan...'
        });

        notif.on('click', () => {
          windowShow(mainWindow);
          mainWindow.webContents.send('notify-click', id);
        });

        notif.on('reply', (event, reply) => {
          mainWindow.webContents.send('notify-reply', id, reply);
        });

        notif.show();
      });
      const extractRealUrl = (urlStr) => {
        const parsed = getUrl(urlStr);
        if (parsed && parsed.hostname === 'l.wl.co') {
          const u = parsed.searchParams.get('u');
          if (u) return u;
        }
        return urlStr;
      };

      const handleOpenUrl = (originalUrl) => {
        const realUrl = extractRealUrl(originalUrl);
        const parsedUrl = getUrl(realUrl);
        if (parsedUrl) {
          const hostname = parsedUrl.hostname;
          if (parsedUrl.protocol === `${urlScheme}:` || hostname === 'chat.whatsapp.com' || hostname === 'api.whatsapp.com' || hostname === 'wa.me') {
            handleWhatsAppProtocol(mainWindow, realUrl);
          } else if (hostname !== 'web.whatsapp.com') {
            shell.openExternal(originalUrl); // open original URL externally for privacy/tracking if intended
          }
        }
      };

      ipcMain.handle("open", (ev, url) => {
        consola.debug("url", url);
        handleOpenUrl(url);
      });
      ipcMain.handle("stateGet", (ev, name) => {
        consola.debug("stateGet", name);
        return state[name];
      });
      ipcMain.handle("windowToggle", () => {
        toggleVisibility(mainWindow);
      });
      ipcMain.handle("escapePressed", () => {
        if (config.get("esc-toggle-window", false)) {
          toggleVisibility(mainWindow);
        }
      });
      ipcMain.handle("insert-text", (ev, text) => {
        console.log("[Main] Menerima request insertText dari WhatsApp:", text);
        if (mainWindow && !mainWindow.isDestroyed()) {
          console.log("[Main] Mengeksekusi mainWindow.webContents.insertText()...");
          mainWindow.webContents.insertText(text);
        } else {
          console.log("[Main] ERROR: mainWindow tidak ada atau hancur");
        }
      });
      ipcMain.handle("log-to-main", (ev, msg) => {
        console.log("[Injected]", msg);
      });
      
      // IPC Handler untuk instal ekstensi
      ipcMain.handle("install-extension", async (ev, extensionId) => {
        try {
          const { installExtension } = await import('electron-extension-installer');
          await installExtension(extensionId, { loadExtensionOptions: { allowFileAccess: true } });
          
          const savedExts = persistState.get("extensions", []);
          if (!savedExts.includes(extensionId)) {
            savedExts.push(extensionId);
            persistState.set("extensions", savedExts);
          }
          consola.info('Installed extension:', extensionId);
          
          dialog.showMessageBox(mainWindow, {
            type: 'info',
            title: 'Ekstensi Terpasang',
            message: 'Ekstensi berhasil dipasang!',
            detail: 'Tutup dan buka kembali WhatsZan (Restart) jika ekstensi belum langsung berfungsi di halaman WhatsApp.'
          });
          
          return true;
        } catch (err) {
          consola.error('Failed to install extension:', err);
          dialog.showErrorBox('Gagal Instal', 'Terjadi kesalahan: ' + err.message);
          return false;
        }
      });

      const updateBlurState = (isMasterOn) => {
        const isContacts = config.get("blur-contacts", true);
        const isPP = config.get("blur-pp", true);
        const isMessages = config.get("blur-messages", true);
        const isMedia = config.get("blur-media", true);
        const isInput = config.get("blur-input", true);
        const specificStr = config.get("blur-specific", "");
        const blurAmount = config.get("blur-amount", 4);

        if (!mainWindow || mainWindow.isDestroyed()) return;

        mainWindow.webContents.executeJavaScript(`
          (() => {
            window.wzIncognito = {
              noRead: ${config.get("incognito-noread", false)},
              noTyping: ${config.get("incognito-notyping", false)},
              antiDelete: ${config.get("incognito-antidelete", false)}
            };
            document.documentElement.style.setProperty('--wz-blur-amount', '${blurAmount}px');
            document.documentElement.style.setProperty('--wz-blur-media', '${blurAmount * 1.5}px');
            document.documentElement.style.setProperty('--wz-unblur-delay', '${config.get("unblur-delay", 0)}s');
            document.body.classList.remove('blur-contacts', 'blur-pp', 'blur-messages', 'blur-media', 'blur-input', 'privacy-blur');
            const oldStyle = document.getElementById('whatszan-blur-specific');
            if (oldStyle) oldStyle.remove();

            if (${isMasterOn}) {
              if (${isPP}) document.body.classList.add('blur-pp');
              if (${isMessages}) document.body.classList.add('blur-messages');
              if (${isMedia}) document.body.classList.add('blur-media');
              if (${isInput}) document.body.classList.add('blur-input');

              const specific = ${JSON.stringify(specificStr || "")};
              const names = specific.split(',').map(n => n.trim()).filter(n => n);
              
              if (names.length > 0) {
                 let css = '';
                 names.forEach(name => {
                   css += \`[title="\${name}"] { filter: blur(var(--wz-blur-amount, 4px)); opacity: 0.75; transform: translateZ(0); transition: filter 0.15s ease-out; }\\n\`;
                   css += \`div[role="row"]:hover [title="\${name}"], div[role="listitem"]:hover [title="\${name}"], header:hover [title="\${name}"], [title="\${name}"]:hover { filter: blur(0px); opacity: 1; transition: filter 0.05s ease-in; }\\n\`;
                 });
                 const style = document.createElement('style');
                 style.id = 'whatszan-blur-specific';
                 style.innerHTML = css;
                 document.head.appendChild(style);
              } else if (${isContacts}) {
                 document.body.classList.add('blur-contacts');
              }
            }
          })();
        `).catch(e => consola.error(e));
      };

      // --- SETTINGS IPC ---
      ipcMain.handle("settings-get", () => {
        return {
          ...config.data,
          "privacy-blur": persistState.get("privacy-blur", false),
          "anti-screencast": persistState.get("anti-screencast", false),
          "sidebar-width": persistState.get("sidebar-width", 220),
          "auto-run": persistState.get("auto-run", false),
          "desktop-shortcut": persistState.get("desktop-shortcut", true),
          "custom-app-name": persistState.get("custom-app-name", "WhatsZan"),
          "icon-choice-app": persistState.get("icon-choice-app", "whatszan"),
          "icon-choice-unread": persistState.get("icon-choice-unread", "whatszan"),
          "icon-choice-normal": persistState.get("icon-choice-normal", "whatszan")
        };
      });

      ipcMain.handle("settings-get-translations", () => {
        const appLang = config.get("app-language", "auto");
        const lang = appLang !== "auto" ? appLang : app.getLocale();
        return loadTranslations(lang);
      });

      ipcMain.handle("settings-save", (ev, newSettings) => {
        const { 'privacy-blur': blur, 'anti-screencast': antiScreencast, 'auto-run': autoRun, 'desktop-shortcut': desktopShortcut, 'custom-app-name': customAppName, ...configSettings } = newSettings;
        
        persistState.set("privacy-blur", blur);
        persistState.set("anti-screencast", antiScreencast);
        persistState.set("auto-run", autoRun);
        
        const oldDesktopShortcut = persistState.get("desktop-shortcut", true);
        const oldAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
        const newAppName = customAppName || "WhatsZan";
        
        persistState.set("desktop-shortcut", desktopShortcut);
        persistState.set("custom-app-name", customAppName);
        
        applyAutoRun(autoRun);
        
        if (oldDesktopShortcut !== desktopShortcut || (desktopShortcut && oldAppName !== newAppName)) {
           applyDesktopShortcut(desktopShortcut, oldAppName, newAppName, config, persistState);
        }

        Object.entries(configSettings).forEach(([k, v]) => {
          if (v !== null && v !== '') config.set(k, v);
          else { config.delete(k); }
        });
        
        updateBlurState(blur);

        // Force taskbar icon refresh
        updateAppUserModel(newAppName, persistState.get("custom_tray_app"));

        // Apply menu bar settings dynamically
        if (!mainWindow.isDestroyed()) {
          mainWindow.setTitle(newAppName);
          
          if (oldAppName !== newAppName) {
            mainWindow.webContents.executeJavaScript(`
              (function() {
                var newName = ${JSON.stringify(newAppName)};
                var oldName = ${JSON.stringify(oldAppName)};
                window.wzAppName = newName;
                
                const processTextNode = (node) => {
                  if (node.nodeValue === oldName || node.nodeValue === oldName + ' Web' || node.nodeValue === 'WhatsApp' || node.nodeValue === 'WhatsApp Web') {
                     let parent = node.parentElement;
                     let isMessage = false;
                     while (parent) {
                       if (parent.getAttribute && (parent.getAttribute('data-testid') === 'msg-container' || parent.getAttribute('role') === 'row' || parent.id === 'main')) {
                         isMessage = true;
                         break;
                       }
                       parent = parent.parentElement;
                     }
                     if (!isMessage) {
                       node.nodeValue = newName;
                     }
                  }
                };
                
                const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null, false);
                let tNode;
                while (tNode = walker.nextNode()) {
                  processTextNode(tNode);
                }
                
                if (newName.toLowerCase() === 'whatsapp') {
                  document.querySelectorAll('.whatszan-logo').forEach(el => el.remove());
                  document.querySelectorAll('svg[aria-label="WhatsApp"]').forEach(svg => svg.style.display = '');
                  document.title = 'WhatsApp Web';
                } else {
                  document.querySelectorAll('.whatszan-logo').forEach(el => el.textContent = newName);
                  document.querySelectorAll('svg[aria-label="WhatsApp"]').forEach(svg => {
                    svg.style.display = 'none';
                    if (svg.parentElement && !svg.parentElement.querySelector('.whatszan-logo')) {
                      const span = document.createElement('span');
                      span.className = 'whatszan-logo';
                      span.textContent = newName;
                      span.style.fontWeight = 'bold';
                      span.style.fontSize = '18px';
                      span.style.color = 'inherit'; 
                      svg.parentElement.appendChild(span);
                    }
                  });
                  if (document.title.includes(oldName) || document.title.includes('WhatsApp')) {
                    document.title = newName;
                  }
                }
              })();
            `).catch(e => {});
          }

          const showMenu = config.get("menu-bar", true);
          const autoHide = config.get("menu-bar-auto-hide", true);
          
          if (showMenu) {
            mainWindow.setMenuBarVisibility(!autoHide);
            mainWindow.autoHideMenuBar = autoHide;
            // Restore menu if it was previously removed
            if (!mainWindow.menuBarVisible && !autoHide) {
                mainWindow.setMenuBarVisibility(true);
            }
          } else {
            mainWindow.removeMenu();
          }
        }

        ev.sender.send("settings-saved");
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.setContentProtection(persistState.get("anti-screencast", false));
          mainWindow.webContents.send("settings-saved");
        }
      });

      ipcMain.handle("branding-upload-icon", async (ev, { type, path: srcPath }) => {
        try {
          const updateLiveIcon = async (t, p) => {
            try {
              const { nativeImage } = await import('electron');
              const img = nativeImage.createFromPath(p);
              if (t === 'app' && mainWindow && !mainWindow.isDestroyed()) mainWindow.setIcon(img);
              if (t === 'normal' && typeof tray !== 'undefined' && tray && !tray.isDestroyed()) tray.setImage(img);
            } catch(e) {}
          };

          persistState.set(`icon-choice-${type}`, srcPath === 'whatszan' || srcPath === 'whatsapp' ? srcPath : 'custom');
          
          if (srcPath === 'whatszan') {
            persistState.delete(`custom_tray_${type}`);
            if (type === 'app') {
               const desktopShortcut = persistState.get("desktop-shortcut", true);
               const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
               applyDesktopShortcut(desktopShortcut, customAppName, customAppName, config, persistState);
               updateAppUserModel(customAppName, path.join(import.meta.dirname, '..', 'build', 'icon.ico'));
               updateLiveIcon('app', path.join(import.meta.dirname, '..', 'build', 'icon.ico'));
            } else if (type === 'normal') {
               updateLiveIcon('normal', path.join(import.meta.dirname, '..', 'static', 'app.png'));
            }
          } else {
            let actualSrcPath = srcPath;
            if (srcPath === 'whatsapp') {
               const ext = type === 'app' ? '.ico' : '.png';
               actualSrcPath = path.join(import.meta.dirname, '..', 'static', `whatsapp${ext}`);
            }
            
            const inputExt = path.extname(actualSrcPath).toLowerCase();
            const isIco = inputExt === '.ico';
            const destExt = type === 'app' ? '.ico' : (isIco ? '.ico' : '.png');
            const destPath = path.join(app.getPath('userData'), `custom_tray_${type}${destExt}`);

            const needsConversion = (type === 'app' && !isIco) || (type !== 'app' && inputExt !== '.png' && !isIco);

            if (needsConversion) {
               import('sharp').then(async ({ default: sharp }) => {
                 try {
                   const size = type === 'app' ? 256 : 32;
                   const pngBuf = await sharp(actualSrcPath).resize(size, size).png().toBuffer();
                   const fs = await import('fs');
                   
                   if (type === 'app') {
                     const header = Buffer.from([0,0,1,0,1,0]);
                     const entry = Buffer.alloc(16);
                     entry.writeUInt8(0, 0); // 0 means 256
                     entry.writeUInt8(0, 1);
                     entry.writeUInt8(0, 2);
                     entry.writeUInt8(0, 3);
                     entry.writeUInt16LE(1, 4);
                     entry.writeUInt16LE(32, 6);
                     entry.writeUInt32LE(pngBuf.length, 8);
                     entry.writeUInt32LE(22, 12);
                     fs.writeFileSync(destPath, Buffer.concat([header, entry, pngBuf]));
                   } else {
                     fs.writeFileSync(destPath, pngBuf);
                   }
                   
                   persistState.set(`custom_tray_${type}`, destPath);
                   updateLiveIcon(type, destPath);
                   if (type === 'app') {
                     const desktopShortcut = persistState.get("desktop-shortcut", true);
                     const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
                     applyDesktopShortcut(desktopShortcut, customAppName, customAppName, config, persistState);
                     updateAppUserModel(customAppName, destPath);
                   }
                 } catch (e) {
                   consola.error("Failed to convert image", e);
                 }
               }).catch(e => consola.error("Sharp module failed to load", e));
            } else {
              import('fs').then(({ copyFileSync }) => {
                copyFileSync(actualSrcPath, destPath);
                persistState.set(`custom_tray_${type}`, destPath);
                updateLiveIcon(type, destPath);
                if (type === 'app') {
                   const desktopShortcut = persistState.get("desktop-shortcut", true);
                   const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
                   applyDesktopShortcut(desktopShortcut, customAppName, customAppName, config, persistState);
                   updateAppUserModel(customAppName, destPath);
                }
              });
            }
          }
        } catch (err) {
          consola.error("Failed to copy branding icon", err);
        }
      });

      ipcMain.handle("branding-reset-icons", async () => {
        persistState.delete("custom_tray_normal");
        persistState.delete("custom_tray_unread");
        persistState.set("icon-choice-normal", "whatszan");
        persistState.set("icon-choice-unread", "whatszan");
        persistState.set("icon-choice-app", "whatszan");
        const hadAppIcon = !!persistState.get("custom_tray_app");
        persistState.delete("custom_tray_app");
        if (hadAppIcon) {
           const desktopShortcut = persistState.get("desktop-shortcut", true);
           const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
           applyDesktopShortcut(desktopShortcut, customAppName, customAppName, config, persistState);
           updateAppUserModel(customAppName, destPath);
        }
        
        try {
           const { nativeImage } = await import('electron');
           if (mainWindow && !mainWindow.isDestroyed()) mainWindow.setIcon(path.join(import.meta.dirname, '..', 'build', 'icon.ico'));
           if (typeof tray !== 'undefined' && tray && !tray.isDestroyed()) tray.setImage(path.join(import.meta.dirname, '..', 'static', 'app.png'));
        } catch(e){}
        return true;
      });

      ipcMain.handle("accounts-get", () => []);
      ipcMain.handle("active-account-get", () => "default");
      ipcMain.handle("account-remove", () => {});

      ipcMain.handle("browse-folder", async (event) => {
        const win = BrowserWindow.fromWebContents(event.sender);
        if (win) win.ignoreBlur = true;
        const result = await dialog.showOpenDialog(win || mainWindow, {
          properties: ['openDirectory'],
        });
        if (win) win.ignoreBlur = false;
        return result.canceled ? null : result.filePaths[0];
      });

      ipcMain.handle("get-default-download-paths", () => ({
        image:    app.getPath('pictures'),
        video:    app.getPath('videos'),
        audio:    app.getPath('music'),
        document: app.getPath('documents'),
      }));

      ipcMain.handle("check-path-exists", (ev, p) => {
        try { return existsSync(p); } catch { return false; }
      });

      ipcMain.on("update-recent-chats", (event, chats) => {
        updateJumpList(chats);
      });

      ipcMain.handle("get-pending-share", () => {
        const share = pendingShare;
        pendingShare = null;
        return share;
      });
      
      ipcMain.handle("read-file-buffer", (event, filepath) => {
        try {
          if (existsSync(filepath)) {
            const buffer = readFileSync(filepath);
            const name = path.basename(filepath);
            return { buffer, name };
          }
        } catch(e) {
          consola.error("Failed to read file buffer", e);
        }
        return null;
      });

      ipcMain.on("update-recent-chats", (ev, chats) => {
        if (process.platform !== 'win32') return;

        const customAppIcon = persistState.get("custom_tray_app");
        const iconPath = customAppIcon || process.execPath;

        const recentCategory = {
          type: 'custom',
          name: 'Obrolan Terbaru',
          items: chats.slice(0, 5).map(chatName => ({
            type: 'task',
            title: chatName,
            program: process.execPath,
            args: `--open-chat="${chatName}"`,
            description: `Buka obrolan dengan ${chatName}`,
            iconPath: iconPath,
            iconIndex: 0
          }))
        };

        const tasksCategory = {
          type: 'tasks',
          items: [
            {
              type: 'task',
              title: 'Obrolan Baru',
              program: process.execPath,
              args: '--action="new-chat"',
              description: 'Mulai obrolan baru',
              iconPath: iconPath,
              iconIndex: 0
            },
            {
              type: 'task',
              title: 'Panggilan Baru',
              program: process.execPath,
              args: '--action="new-call"',
              description: 'Mulai panggilan baru',
              iconPath: iconPath,
              iconIndex: 0
            }
          ]
        };

        try {
          app.setJumpList([recentCategory, tasksCategory]);
        } catch (err) {
          consola.error("Failed to set jump list", err);
        }
      });

      ipcMain.handle("open-webstore", () => {
        const extWin = new BrowserWindow({
          width: 1024,
          height: 768,
          title: 'Chrome Web Store',
          webPreferences: {
            nodeIntegration: false,
            contextIsolation: true,
            preload: path.join(import.meta.dirname, '..', 'src-web', 'webstore-preload.js'),
          }
        });
        extWin.setMenu(null);
        const cleanUA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36';
        extWin.webContents.setUserAgent(cleanUA);
        extWin.loadURL('https://chromewebstore.google.com/');
      });
      // --- END SETTINGS IPC ---

      const openSettings = async () => {
        const customAppIcon = persistState.get("custom_tray_app");
        const setWin = new BrowserWindow({
          width: 720,
          height: 600,
          title: 'Pengaturan WhatsZan',
          icon: customAppIcon || path.join(import.meta.dirname, '..', 'build', 'icon.ico'),
          autoHideMenuBar: true,
          resizable: false,
          webPreferences: {
            nodeIntegration: true,
            contextIsolation: false,
          }
        });
        setWin.setMenu(null);

        let isWaDark = nativeTheme.shouldUseDarkColors;
        try {
          if (mainWindow && !mainWindow.isDestroyed()) {
            isWaDark = await mainWindow.webContents.executeJavaScript(`
              document.documentElement.classList.contains('dark') || document.body.classList.contains('dark')
            `);
          }
        } catch (e) {}

        setWin.loadFile(path.join(import.meta.dirname, '..', 'static', 'settings.html'), {
          query: { theme: isWaDark ? 'dark' : 'light' }
        });

        setWin.on('blur', () => {
          if (setWin.ignoreBlur) return;
          if (!setWin.isDestroyed()) setWin.close();
        });
      };

      const trayIcon = persistState.get("custom_tray_normal") || getUserIcon("app", state) || path.join(import.meta.dirname, "..", "static", "app.png");
      let tray = setupTray(trayIcon, translations, mainWindow, openSettings);

      setupAppMenu(mainWindow, openSettings, persistState, updateBlurState);


      const notif = (options) => {
        const n = new Notification({
          title: pkg.name,
          ...options,
        });
        n.show();
        return n;
      };

      async function executeScript(data) {
        try {
          await mainWindow.webContents.executeJavaScript(data);
        } catch (err) {
          consola.error("executeJavaScript", err);
        }
      }
      mainWindow.webContents.on("did-finish-load", async (ev) => {
        consola.debug("did-finish-load");
        // builtin scripts
        const scripts = ["injected.js"];
        consola.debug("load scripts", scripts);
        for (const script of scripts) {
          const filename = path.join(import.meta.dirname, "..", "src-web", script);
          let data = readFileSync(filename, "utf-8");
          if (script === "injected.js") {
            const customAppName = persistState.get("custom-app-name", "WhatsZan") || "WhatsZan";
            data = `window.wzAppName = ${JSON.stringify(customAppName)};\n` + data;
          }
          await executeScript(data);
        }

        // builtin CSS
        try {
          const blurCssFile = path.join(import.meta.dirname, "..", "src-web", "privacy-blur.css");
          const blurCssData = readFileSync(blurCssFile, "utf-8");
          mainWindow.webContents.insertCSS(blurCssData);
        } catch(e) {
          consola.error("error loading privacy-blur.css", e);
        }

        updateBlurState(persistState.get("privacy-blur", false));

        // user scripts
        const userScriptsPath = path.resolve(path.join(app.getPath("userData"), "user-scripts"));
        for (let filename of config.get("scripts", [])) {
          filename = replaceVariables(filename);
          filename = path.resolve(filename);
          consola.info("load user script", filename);
          if (!filename.startsWith(userScriptsPath)) {
            consola.error(`script must be in ${userScriptsPath}:`, filename);
            continue;
          }
          if (!filename.endsWith(".js")) {
            consola.error("script must end with .js", filename);
            continue;
          }
          const data = readFileSync(filename, "utf-8");
          if (data) {
            await executeScript(data);
          }
        }
        for (const css of config.get("css", [])) {
          try {
            let url = getUrl(css);
            let data = null;
            if (url) {
              data = await loadUrl(url);
            } else {
              data = css;
            }

            if (data) {
              mainWindow.webContents.insertCSS(data);
            }
          } catch (err) {
            consola.error(`error inserting ${css}`, err);
          }
        }
      });

      let newestIcon = null;
      mainWindow.webContents.on("page-favicon-updated", async (ev, favicons) => {
        if (favicons.length > 0) {
          const lastFaviconUrl = favicons[favicons.length - 1];
          newestIcon = lastFaviconUrl;
          const img = await getIcon(lastFaviconUrl, state);
          // test that the icon corresponds to the last emitted event
          // sometimes an old icon takes time to download and arrives late
          if (img && lastFaviconUrl === newestIcon) {
            // we could also extract it from the page title, may be more reliable
            const unreadCount = getUnreadCountFromFavicon(lastFaviconUrl);
            app.setBadgeCount(unreadCount); // libunity (Linux & macOS only)

            dbus?.setBadgeCount(unreadCount); // gnome, kde

            const customUnread = persistState.get("custom_tray_unread");
            let iconToPass = trayIcon;
            let drawBadge = unreadCount;

            if (unreadCount > 0 && customUnread) {
              iconToPass = customUnread;
              drawBadge = 0; // Don't draw the red dot over custom unread icon
            }

            getBadgedTrayIcon(iconToPass, drawBadge).then(icon => {
              if (icon && lastFaviconUrl === newestIcon) {
                tray?.setImage(icon);
              }
            });
          }
        }
      });

      const url = config.get("url", "https://web.whatsapp.com/");

      mainWindow.webContents.setWindowOpenHandler(({ url }) => {
        if (url === 'about:blank' || url.startsWith('https://web.whatsapp.com/')) {
          return { 
            action: 'allow', 
            overrideBrowserWindowOptions: {
              autoHideMenuBar: true,
            }
          };
        }
        handleOpenUrl(url);
        return { action: "deny" };
      });

      mainWindow.webContents.on("will-navigate", (event, url) => {
        const realUrl = extractRealUrl(url);
        const parsedUrl = getUrl(realUrl);
        if (parsedUrl && parsedUrl.hostname !== 'web.whatsapp.com') {
          event.preventDefault();
          handleOpenUrl(url);
        }
      });

      mainWindow.webContents.on("did-fail-load", async (ev) => {
        consola.warn("did-fail-load");
        notif({
          body: "Failed to load",
        }).on("click", () => {
          windowShow(mainWindow);
        });

        setTimeout(
          () => {
            consola.info("retry");
            mainWindow.webContents.loadURL(url);
          },
          config.get("retry-interval", 15000),
        );
      });

      if (isDebug) {
        mainWindow.webContents.loadFile("static/debug.html");
      } else {
        mainWindow.webContents.loadURL(url);
      }
    });
    return mainWindow;
  };

  function handleJumpListArgs(window, commandLine) {
    const openChatArg = commandLine.find(arg => arg.startsWith('--open-chat='));
    if (openChatArg) {
      const chatName = openChatArg.substring('--open-chat='.length).replace(/^"|"$/g, '');
      windowShow(window);
      window.webContents.send('jump-list-action', { type: 'open-chat', name: chatName });
      return true;
    }

    const actionArg = commandLine.find(arg => arg.startsWith('--action='));
    if (actionArg) {
      const actionType = actionArg.substring('--action='.length).replace(/^"|"$/g, '');
      windowShow(window);
      window.webContents.send('jump-list-action', { type: 'action', name: actionType });
      return true;
    }
    
    if (handleShareArgs(window, commandLine)) {
      return true;
    }

    return false;
  }

  app.setAboutPanelOptions({
    applicationName: pkg.name,
    applicationVersion: app.getVersion(),
    authors: [pkg?.author?.name],
    website: pkg?.homepage,
    // iconPath: "static/app.png", does not work when packaged
    copyright: pkg?.license,
  });
  // This method will be called when Electron has finished
  // initialization and is ready to create browser windows.
  // Some APIs can only be used after this event occurs.
  
  const appLang = config.get("app-language", "auto");
  if (appLang !== "auto") {
    app.commandLine.appendSwitch('lang', appLang);
  }

  app.whenReady().then(async () => {
    
    if (process.platform === 'win32') {
      try {
        const customAppIcon = persistState.get("custom_tray_app");
        app.setUserTasks([
          {
            program: process.execPath,
            arguments: '--action="new-chat"',
            iconPath: customAppIcon || process.execPath,
            iconIndex: 0,
            title: translations.new_chat || 'Chat Baru',
            description: translations.new_chat_desc || 'Mulai obrolan baru'
          }
        ]);
      } catch (err) {}
    }

    let window = await createWindow();    // Otomatis arahkan folder unduhan berdasarkan jenis file
    session.defaultSession.on('will-download', (event, item) => {
      if (config.get('smart-download', true) === false) return;

      const filename = item.getFilename();
      const ext = path.extname(filename).toLowerCase();
      
      let targetDir = app.getPath('downloads');
      
      if (['.mp4', '.mkv', '.avi', '.mov', '.wmv', '.webm'].includes(ext)) {
        targetDir = config.get('download-path-video', '') || app.getPath('videos');
      } else if (['.jpg', '.jpeg', '.png', '.gif', '.webp', '.bmp', '.svg'].includes(ext)) {
        targetDir = config.get('download-path-image', '') || app.getPath('pictures');
      } else if (['.mp3', '.wav', '.ogg', '.m4a', '.flac'].includes(ext)) {
        targetDir = config.get('download-path-audio', '') || app.getPath('music');
      } else if (['.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx', '.txt', '.zip', '.rar', '.7z'].includes(ext)) {
        targetDir = config.get('download-path-document', '') || app.getPath('documents');
      }

      item.setSaveDialogOptions({
        defaultPath: path.join(targetDir, filename)
      });
    });

    // Check if app was launched with a whatsapp: URL scheme
    const whatsappUrl = process.argv.find((arg) => arg.startsWith(`${urlScheme}:`));
    if (whatsappUrl) {
      handleWhatsAppProtocol(window, whatsappUrl);
    } else {
      handleJumpListArgs(window, process.argv);
      handleShareArgs(window, process.argv);
    }


    addAboutMenuItem();
    app.on("activate", async () => {
      // On macOS it's common to re-create a window in the app when the
      // dock icon is clicked and there are no other windows open.
      if (BrowserWindow.getAllWindows().length === 0) {
        window = await createWindow();
      }
    });

    app.on("second-instance", (event, commandLine, workingDirectory, additionalData) => {
      consola.debug("second-instance", additionalData, commandLine);

      if (window) {
        // Check for whatsapp: protocol URLs
        const whatsappUrl = commandLine.find((arg) => arg.startsWith(`${urlScheme}:`));
        if (whatsappUrl) {
          handleWhatsAppProtocol(window, whatsappUrl);
          return;
        }
        
        if (handleJumpListArgs(window, commandLine)) {
          return;
        }
        
        if (handleShareArgs(window, commandLine)) {
          return;
        }

        if (commandLine.includes("--hide")) {
          window.hide();
        } else if (commandLine.includes("--toggle")) {
          if (window.isVisible()) {
            window.hide();
          } else {
            windowShow(window);
          }
        } else if (commandLine.includes("--quit")) {
          app.quit();
        } else {
          // --show
          windowShow(window);
        }
      }
    });
  });

  // Quit when all windows are closed, except on macOS. There, it's common
  // for applications and their menu bar to stay active until the user quits
  // explicitly with Cmd + Q.
  app.on("window-all-closed", () => {
    consola.debug("window-all-closed");
    if (process.platform !== "darwin") app.quit();
  });
}

const gotTheLock = app.requestSingleInstanceLock({ name: pkg.name });

if (!gotTheLock) {
  consola.info("already running, raised the main window");
  app.quit();
} else {
  main();
}
