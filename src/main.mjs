import { app, BrowserWindow, session, Menu, MenuItem, Tray, ipcMain, shell, Notification, dialog, nativeTheme } from "electron";
import { readFileSync } from "node:fs";
import fs from "node:fs";
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


  const createWindow = async () => {
    // Create the browser window.
    const activeAccount = persistState.get("active-account", "default");
    const partitionString = activeAccount === "default" ? undefined : `persist:${activeAccount}`;

    const mainWindow = new BrowserWindow({
      webPreferences: {
        preload: path.join(import.meta.dirname, "..", "src-web", "preload.js"),
        spellcheck: config.get("spellcheck", true),
        partition: partitionString,
      },
      show: state.showAtStartup,
      autoHideMenuBar: config.get("menu-bar-auto-hide", true),
      ...state.windowBounds,
    });

    if (!config.get("menu-bar", true)) {
      mainWindow.removeMenu();
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
      mainWindow.webContents.session.setSpellCheckerLanguages(spellLangs);
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

    mainWindow.webContents.session.webRequest.onBeforeSendHeaders((details, callback) => {
      details.requestHeaders["User-Agent"] = state.userAgent;
      callback({ cancel: false, requestHeaders: details.requestHeaders });
    });

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
      
      // Apply last known theme immediately to prevent flash on QR page
      const lastTheme = persistState.get("last-theme-source", null);
      if (lastTheme) nativeTheme.themeSource = lastTheme;

      monitorSystemTheme(nativeTheme, (source) => {
        persistState.set("last-theme-source", source);
      });

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
      ipcMain.handle("isDarkMode", () => {
        return nativeTheme.shouldUseDarkColors;
      });



      // Settings IPC
      let settingsWindow = null;
      const openSettingsWindow = () => {
        if (settingsWindow && !settingsWindow.isDestroyed()) {
          settingsWindow.focus();
          return;
        }
        settingsWindow = new BrowserWindow({
          width: 480, height: 700,
          parent: mainWindow, modal: false,
          resizable: false, minimizable: false, maximizable: false,
          autoHideMenuBar: true,
          title: 'Pengaturan',
          webPreferences: {
            nodeIntegration: true,
            contextIsolation: false,
          },
        });
        settingsWindow.loadFile(path.join(import.meta.dirname, '..', 'static', 'settings.html'));
        settingsWindow.on('closed', () => { settingsWindow = null; });
      };

      ipcMain.handle('settings-get', () => {
        return {
          'notification-prefix': config.get('notification-prefix', ''),
          'auto-run': persistState.get('auto-run', false),
          'hide-on-start': !config.get('show-at-startup', true),
          'quit-on-close': config.get('quit-on-close', false),
          'esc-toggle-window': config.get('esc-toggle-window', false),
          'spellcheck': config.get('spellcheck', true),
          'menu-bar': config.get('menu-bar', true),
          'menu-bar-auto-hide': config.get('menu-bar-auto-hide', true),
          'user-agent': config.get('user-agent', ''),
        };
      });

      ipcMain.handle('accounts-get', () => persistState.get('accounts', []));
      ipcMain.handle('active-account-get', () => persistState.get('active-account', 'default'));

      ipcMain.handle('account-remove', (ev, id) => {
        const accounts = persistState.get('accounts', []).filter(a => a.id !== id);
        persistState.set('accounts', accounts);
        // If removed account is currently active, switch to default
        if (persistState.get('active-account', 'default') === id) {
          persistState.set('active-account', 'default');
          app.relaunch();
          app.quit();
        }
      });

      ipcMain.handle('account-add', () => {
        const accounts = persistState.get('accounts', []);
        const newId = `account_${Date.now()}`;
        const newName = `Akun ${accounts.length + 2}`;
        // Mark as pending — will only be permanently saved after QR scan
        persistState.set('pending-account', { id: newId, name: newName });
        persistState.set('active-account', newId);
        app.relaunch();
        app.quit();
      });

      ipcMain.handle('settings-save', (ev, newSettings) => {
        config.set('notification-prefix', newSettings['notification-prefix'] || '');
        config.set('show-at-startup', !newSettings['hide-on-start']);
        config.set('quit-on-close', newSettings['quit-on-close']);
        config.set('esc-toggle-window', newSettings['esc-toggle-window']);
        config.set('spellcheck', newSettings['spellcheck']);
        config.set('menu-bar', newSettings['menu-bar']);
        config.set('menu-bar-auto-hide', newSettings['menu-bar-auto-hide']);
        if (newSettings['user-agent']) config.set('user-agent', newSettings['user-agent']);
        else config.delete('user-agent');
        const autoRun = newSettings['auto-run'];
        persistState.set('auto-run', autoRun);
        app.setLoginItemSettings({
          openAtLogin: autoRun,
          args: autoRun ? ['--hide'] : []
        });
        if (settingsWindow && !settingsWindow.isDestroyed()) {
          settingsWindow.webContents.send('settings-saved');
        }
        setTimeout(() => { app.relaunch(); app.quit(); }, 1200);
      });

      const trayIcon = getUserIcon("app", state) || path.join(import.meta.dirname, "..", "static", "app.png");
      const tray = new Tray(trayIcon);
      
      const activeAccount = persistState.get("active-account", "default");

      const switchAccountHandler = (accountId) => {
        if (persistState.get("active-account", "default") !== accountId) {
          persistState.set("active-account", accountId);
          app.relaunch();
          app.quit();
        }
      };

      const buildAccountSubmenu = () => {
        const current = persistState.get("active-account", "default");
        const items = [
          { label: translations?.account_main ?? "Akun Utama", type: "radio", checked: current === "default", click: () => switchAccountHandler("default") },
          { label: "Akun 2", type: "radio", checked: current === "account_2", click: () => switchAccountHandler("account_2") },
          { label: "Akun 3", type: "radio", checked: current === "account_3", click: () => switchAccountHandler("account_3") }
        ];
        return items;
      };

      const updateMenus = () => {
        const accountSubmenu = buildAccountSubmenu();
        
        // Update Tray Menu
        const trayContextMenu = Menu.buildFromTemplate([
          {
            label: translations?.show_hide ?? "Show/Hide",
            type: "normal",
            click: () => toggleVisibility(mainWindow),
          },
          {
            label: translations?.switch_account ?? "Switch Account",
            submenu: accountSubmenu
          },
          { type: "separator" },
          {
            label: translations?.settings ?? "Settings",
            click: () => openSettingsWindow(),
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

        const current = persistState.get("active-account", "default");
        let accountName = translations?.account_main ?? "Akun Utama";
        if (current === "account_2") accountName = "Akun 2";
        if (current === "account_3") accountName = "Akun 3";

        tray.setToolTip(`whatszan - ${accountName}`);
        if (process.platform !== 'win32') {
          tray.setTitle(` ${accountName}`);
        }
        tray.setContextMenu(trayContextMenu);

        // Update App Menu
        const appMenu = Menu.getApplicationMenu();
        if (appMenu) {
          const windowMenu = appMenu.items.find(item => item.role === 'windowmenu' || item.label === 'Window' || item.label === 'Jendela');
          if (windowMenu) {
            const switchAccountItem = windowMenu.submenu.items.find(item => item.label === (translations?.switch_account ?? "Switch Account"));
            if (switchAccountItem) {
              // Replace the entire Application Menu by rebuilding it from the current template
              // but since we don't have the full template easily, we can just replace the App Menu completely
              // Wait, Electron doesn't allow dynamic submenu mutation on Mac/Linux sometimes, 
              // but we can try setting the submenu property, or just rebuilding the AppMenu.
              // Actually, since we already built `appMenu` initially, we can just set its items.
              // Electron allows re-setting the application menu:
              const template = appMenu.items.map(item => {
                if (item.role === 'windowmenu' || item.label === 'Window' || item.label === 'Jendela') {
                  const subItems = item.submenu.items.map(sub => {
                    if (sub.label === (translations?.switch_account ?? "Switch Account")) {
                      return { label: sub.label, submenu: buildAccountSubmenu() };
                    }
                    if (sub.id === 'auto_run_appmenu') return { id: sub.id, label: sub.label, type: sub.type, checked: sub.checked, click: sub.click };
                    if (sub.id === 'settings_appmenu') return { id: sub.id, label: sub.label, accelerator: sub.accelerator, click: sub.click };
                    return sub.type === 'separator' ? { type: 'separator' } : { label: sub.label, role: sub.role, click: sub.click };
                  });
                  return { role: item.role, label: item.label, submenu: subItems };
                }
                return { role: item.role, label: item.label, submenu: item.submenu };
              });
              Menu.setApplicationMenu(Menu.buildFromTemplate(template));
            }
          }
        }
      };

      updateMenus();


      tray.on("click", () => {
        toggleVisibility(mainWindow);
      });

      const appMenu = Menu.getApplicationMenu();
      if (appMenu) {
        const windowMenu = appMenu.items.find(item => item.role === 'windowmenu' || item.label === 'Window' || item.label === 'Jendela');
        if (windowMenu && !windowMenu.submenu.items.some(i => i.id === 'auto_run_appmenu')) {
          windowMenu.submenu.append(new MenuItem({ type: 'separator' }));
          
          windowMenu.submenu.append(new MenuItem({
            label: translations?.switch_account ?? "Switch Account",
            submenu: Menu.buildFromTemplate(buildAccountSubmenu())
          }));

          windowMenu.submenu.append(new MenuItem({
            id: 'auto_run_appmenu',
            label: translations?.auto_run ?? "Auto Run & Minimize",
            type: "checkbox",
            checked: persistState.get("auto-run", false),
            click: (menuItem) => {
              const isChecked = menuItem.checked;
              persistState.set("auto-run", isChecked);
              app.setLoginItemSettings({
                openAtLogin: isChecked,
                args: isChecked ? ["--hide"] : []
              });
            }
          }));

          windowMenu.submenu.append(new MenuItem({ type: 'separator' }));
          windowMenu.submenu.append(new MenuItem({
            id: 'settings_appmenu',
            label: translations?.settings ?? "Settings",
            accelerator: 'Alt+S',
            click: () => openSettingsWindow(),
          }));
        }
      }

      ipcMain.handle("getActiveAccountName", () => {
        const current = persistState.get("active-account", "default");
        if (current === "default") return translations?.account_main ?? "Akun Utama";
        if (current === "account_2") return "Akun 2";
        if (current === "account_3") return "Akun 3";
        return translations?.account_main ?? "Akun Utama";
      });


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
      // Track injected CSS key for dynamic theme updates
      let injectedThemeCssKey = null;

      const applyThemeCss = async () => {
        if (injectedThemeCssKey) {
          try { await mainWindow.webContents.removeInsertedCSS(injectedThemeCssKey); } catch(e) {}
          injectedThemeCssKey = null;
        }
        if (nativeTheme.shouldUseDarkColors) {
          const darkCSS = `
            body:not(.dark) { background-color: #0d1418 !important; }
            body:not(.dark) #app { filter: invert(1) hue-rotate(180deg); }
            body:not(.dark) img, body:not(.dark) video, body:not(.dark) canvas { filter: invert(1) hue-rotate(180deg); }
          `;
          try { injectedThemeCssKey = await mainWindow.webContents.insertCSS(darkCSS); } catch(e) {}
        }
      };

      mainWindow.webContents.on("dom-ready", async () => {
        await applyThemeCss();
      });

      nativeTheme.on("updated", async () => {
        await applyThemeCss();
      });


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
            app.setBadgeCount(unreadCount); // libunity
            dbus?.setBadgeCount(unreadCount); // gnome, kde

            getBadgedTrayIcon(trayIcon, unreadCount).then(icon => {
              if (icon && lastFaviconUrl === newestIcon) {
                tray.setImage(icon);
              }
            });
          }
        }
      });

      const url = config.get("url", "https://web.whatsapp.com/");

      mainWindow.webContents.setWindowOpenHandler(({ url }) => {
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
