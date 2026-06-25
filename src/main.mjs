import { app, BrowserWindow, session, Menu, MenuItem, Tray, ipcMain, shell, Notification, dialog, nativeTheme, desktopCapturer } from "electron";
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

const defaultUserAgent =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

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

  const state = {
    notifPrefix: config.get("notification-prefix", ""),
    showAtStartup: (isDebug || config.get("show-at-startup", true)) && !isHiddenStartup,
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
    const mainWindow = new BrowserWindow({
      webPreferences: {
        preload: path.join(import.meta.dirname, "..", "src-web", "preload.js"),
        spellcheck: config.get("spellcheck", true),
      },
      show: state.showAtStartup,
      autoHideMenuBar: config.get("menu-bar-auto-hide", true),
      ...state.windowBounds,
    });

    if (!config.get("menu-bar", true)) {
      mainWindow.removeMenu();
    }


    // Override navigator.userAgent di JavaScript context (bukan hanya HTTP header).
    // WhatsApp Web Beta mengecek navigator.userAgent via JS untuk mengaktifkan
    // fitur seperti panggilan suara/video. Tanpa ini, Windows terdeteksi dan
    // fitur beta tidak aktif meski HTTP header sudah di-spoof.
    mainWindow.webContents.setUserAgent(state.userAgent);

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

    const lang = preferredLangs[0];
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
          title: 'Bagikan Layar',
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

        pickerWin.loadFile(path.join(import.meta.dirname, '..', 'static', 'screen-picker.html'), {
          query: { theme: isWaDark ? 'dark' : 'light' },
        });

        // Kirim daftar sumber ke picker setelah halaman siap
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
      if (!isDebug) {
        persistState.set("window-bounds", mainWindow.getBounds());
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
        const specificStr = config.get("blur-specific", "");

        if (!mainWindow || mainWindow.isDestroyed()) return;

        mainWindow.webContents.executeJavaScript(`
          (() => {
            document.body.classList.remove('blur-contacts', 'blur-pp', 'blur-messages', 'blur-media', 'privacy-blur');
            const oldStyle = document.getElementById('whatszan-blur-specific');
            if (oldStyle) oldStyle.remove();

            if (${isMasterOn}) {
              if (${isPP}) document.body.classList.add('blur-pp');
              if (${isMessages}) document.body.classList.add('blur-messages');
              if (${isMedia}) document.body.classList.add('blur-media');

              const specific = ${JSON.stringify(specificStr || "")};
              const names = specific.split(',').map(n => n.trim()).filter(n => n);
              
              if (names.length > 0) {
                 let css = '';
                 names.forEach(name => {
                   css += \`[title="\${name}"] { filter: blur(4px); opacity: 0.75; transform: translateZ(0); transition: filter 0.15s ease-out; }\\n\`;
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
          "auto-run": persistState.get("auto-run", false)
        };
      });

      ipcMain.handle("settings-save", (ev, newSettings) => {
        const { 'privacy-blur': blur, 'auto-run': autoRun, ...configSettings } = newSettings;
        
        persistState.set("privacy-blur", blur);
        persistState.set("auto-run", autoRun);
        
        if (process.platform === 'win32') {
          const shortcutPath = path.join(process.env.APPDATA || '', 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup', 'WhatsZan.lnk');
          if (autoRun) {
            const exePath = app.getPath('exe');
            const workDir = path.dirname(exePath);
            const ps = `$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('${shortcutPath}'); $s.TargetPath = '${exePath}'; $s.Arguments = '--hide'; $s.WorkingDirectory = '${workDir}'; $s.Save()`;
            import('child_process').then(({ execSync }) => {
              try { execSync(`powershell -NoProfile -Command "${ps}"`, { stdio: 'ignore' }); } catch(e){}
            });
          } else {
            try { if (existsSync(shortcutPath)) unlinkSync(shortcutPath); } catch(e){}
          }
        } else {
          app.setLoginItemSettings({ openAtLogin: autoRun, args: autoRun ? ['--hide'] : [] });
        }

        Object.entries(configSettings).forEach(([k, v]) => {
          if (v !== null && v !== '') config.set(k, v);
          else { config.delete(k); }
        });

        
        updateBlurState(blur);

        ev.sender.send("settings-saved");
      });

      ipcMain.handle("accounts-get", () => []);
      ipcMain.handle("active-account-get", () => "default");
      ipcMain.handle("account-add", () => {});
      ipcMain.handle("account-remove", () => {});

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

      const trayIcon = getUserIcon("app", state) || path.join(import.meta.dirname, "..", "static", "app.png");
      let tray;
      try {
        tray = new Tray(trayIcon);
        const trayContextMenu = Menu.buildFromTemplate([
          {
            label: translations?.show_hide ?? "Show/Hide",
            type: "normal",
            click: () => {
              toggleVisibility(mainWindow);
            },
          },
          {
            label: translations?.quit ?? "Quit",
            type: "normal",
            click: () => {
              consola.debug("quit");
              app.isQuiting = true;
              app.quit();
            },
          },
        ]);
        tray.setToolTip(pkg.name);
        tray.setContextMenu(trayContextMenu);
        tray.on("click", () => {
          toggleVisibility(mainWindow);
        });
      } catch (err) {
        consola.error("Failed to load tray icon", err);
      }

      const appMenu = Menu.getApplicationMenu();
      if (appMenu) {
        const windowMenu = appMenu.items.find(item => item.role === 'windowmenu' || item.label === 'Window' || item.label === 'Jendela');
        if (windowMenu && !windowMenu.submenu.items.some(i => i.id === 'auto_run_appmenu')) {

          // Helper: path ke shortcut di folder Startup Windows
          const getStartupShortcutPath = () => path.join(
            process.env.APPDATA || '',
            'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup',
            'WhatsZan.lnk'
          );

          // Helper: baca status auto-run saat ini (per-OS)
          const isAutoRunEnabled = () => {
            if (process.platform === 'win32') {
              return existsSync(getStartupShortcutPath());
            }
            return app.getLoginItemSettings().openAtLogin;
          };

          // Helper: set/hapus auto-run (per-OS)
          const setAutoRun = (enabled) => {
            if (process.platform === 'win32') {
              const shortcutPath = getStartupShortcutPath();
              if (enabled) {
                // Buat shortcut .lnk di folder Startup via PowerShell
                const exePath = app.getPath('exe');
                const workDir = path.dirname(exePath);
                const ps = [
                  `$ws = New-Object -ComObject WScript.Shell`,
                  `$s = $ws.CreateShortcut('${shortcutPath}')`,
                  `$s.TargetPath = '${exePath}'`,
                  `$s.Arguments = '--hide'`,
                  `$s.WorkingDirectory = '${workDir}'`,
                  `$s.Save()`,
                ].join('; ');
                import('child_process').then(({ execSync }) => {
                  try { execSync(`powershell -NoProfile -Command "${ps}"`, { stdio: 'ignore' }); }
                  catch (e) { consola.error('AutoRun shortcut error', e); }
                });
              } else {
                // Hapus shortcut
                try { if (existsSync(shortcutPath)) unlinkSync(shortcutPath); }
                catch (e) { consola.error('AutoRun remove shortcut error', e); }
              }
            } else {
              // Linux/macOS: gunakan app.setLoginItemSettings (registry/launchd)
              app.setLoginItemSettings({
                openAtLogin: enabled,
                args: enabled ? ['--hide'] : []
              });
            }
          };

          windowMenu.submenu.append(new MenuItem({ type: 'separator' }));
          
          windowMenu.submenu.append(new MenuItem({
            id: 'settings_appmenu',
            label: "Pengaturan...",
            accelerator: 'CmdOrCtrl+,',
            click: async () => {
              const setWin = new BrowserWindow({
                width: 720,
                height: 600,
                title: 'Pengaturan WhatsZan',
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

              // Tutup otomatis saat kehilangan fokus
              setWin.on('blur', () => {
                if (!setWin.isDestroyed()) setWin.close();
              });
            }
          }));

          // Tetap pertahankan shortcut untuk Privacy Blur tanpa harus menampilkannya di menu
          import('electron').then(({ globalShortcut }) => {
            globalShortcut.register('CommandOrControl+Shift+B', () => {
              const current = persistState.get("privacy-blur", false);
              const newState = !current;
              persistState.set("privacy-blur", newState);
              updateBlurState(newState);
            });
          });
        }
      }


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
          const data = readFileSync(filename, "utf-8");
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

            getBadgedTrayIcon(trayIcon, unreadCount).then(icon => {
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
  app.whenReady().then(async () => {
    let window = await createWindow();

    // Otomatis arahkan folder unduhan berdasarkan jenis file
    session.defaultSession.on('will-download', (event, item) => {
      if (config.get('smart-download', true) === false) return;

      const filename = item.getFilename();
      const ext = path.extname(filename).toLowerCase();
      
      let targetDir = app.getPath('downloads');
      
      if (['.mp4', '.mkv', '.avi', '.mov', '.wmv', '.webm'].includes(ext)) {
        targetDir = app.getPath('videos');
      } else if (['.jpg', '.jpeg', '.png', '.gif', '.webp', '.bmp', '.svg'].includes(ext)) {
        targetDir = app.getPath('pictures');
      } else if (['.mp3', '.wav', '.ogg', '.m4a', '.flac'].includes(ext)) {
        targetDir = app.getPath('music');
      } else if (['.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx', '.txt', '.zip', '.rar', '.7z'].includes(ext)) {
        targetDir = app.getPath('documents');
      }

      item.setSaveDialogOptions({
        defaultPath: path.join(targetDir, filename)
      });
    });

    // Check if app was launched with a whatsapp: URL scheme
    const whatsappUrl = process.argv.find((arg) => arg.startsWith(`${urlScheme}:`));
    if (whatsappUrl) {
      handleWhatsAppProtocol(window, whatsappUrl);
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
