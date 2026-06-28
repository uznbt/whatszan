import { Tray, Menu, app } from "electron";
import { consola } from "consola";
import pkg from "../package.json" with { type: "json" };
import { toggleVisibility } from "./util.mjs";

export const setupTray = (trayIcon, translations, mainWindow, openSettings) => {
  let tray = null;
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
        label: "Pengaturan...",
        type: "normal",
        click: () => {
          openSettings();
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
  return tray;
};
