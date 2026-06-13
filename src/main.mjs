import { app, BrowserWindow, session, Menu, MenuItem, Tray, ipcMain, shell, Notification, dialog, nativeTheme } from "electron";
import { readFileSync } from "node:fs";
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

      const trayIcon = getUserIcon("app", state) || path.join(import.meta.dirname, "..", "static", "app.png");
      const tray = new Tray(trayIcon);
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

      const appMenu = Menu.getApplicationMenu();
      if (appMenu) {
        const windowMenu = appMenu.items.find(item => item.role === 'windowmenu' || item.label === 'Window' || item.label === 'Jendela');
        if (windowMenu && !windowMenu.submenu.items.some(i => i.id === 'auto_run_appmenu')) {
          windowMenu.submenu.append(new MenuItem({ type: 'separator' }));
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
