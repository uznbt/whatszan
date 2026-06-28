import { Tray, Menu, app } from "electron";
import { consola } from "consola";
import pkg from "../package.json" with { type: "json" };
import { toggleVisibility } from "./util.mjs";

export const setupTray = (trayIcon, translations, mainWindow, openSettings, initialDND, onToggleDND) => {
  let tray = null;
  try {
    tray = new Tray(trayIcon);

    const buildMenu = (isDND) => Menu.buildFromTemplate([
      {
        label: translations?.show_hide ?? "Show/Hide",
        type: "normal",
        click: () => {
          toggleVisibility(mainWindow);
        },
      },
      {
        label: translations?.settings ?? "Pengaturan...",
        type: "normal",
        click: () => {
          openSettings();
        },
      },
      { type: "separator" },
      {
        label: translations?.dnd_mode ?? "Mode Jangan Ganggu (DND)",
        type: "checkbox",
        checked: isDND,
        click: (menuItem) => {
          onToggleDND(menuItem.checked);
          tray.setContextMenu(buildMenu(menuItem.checked));
        },
      },
      { type: "separator" },
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
    tray.setContextMenu(buildMenu(initialDND));
    tray.on("click", () => {
      toggleVisibility(mainWindow);
    });
  } catch (err) {
    consola.error("Failed to load tray icon", err);
  }
  return tray;
};
