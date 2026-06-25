import * as os from "node:os";
import * as path from "node:path";
import { app } from "electron";
import { toggleVisibility, windowShow } from "./util.mjs";
import { consola } from "consola";


const SERVICE_NAME = "org.uznbt.whatszan";
const OBJECT_PATH = "/" + SERVICE_NAME.replaceAll(".", "/");

export class Dbus {
  sessionBus = null;
  window = null;
  serialCounter = 0;
  dbusRef = null; // referensi ke modul dbus-native setelah dynamic import

  constructor(window) {
    if (os.platform() !== "linux") {
      return;
    }
    this.window = window;

    // Dynamic import agar tidak crash di Windows jika modul opsional tidak tersedia
    import("@homebridge/dbus-native").then((mod) => {
      const dbus = mod.default;
      this.dbusRef = dbus;
      try {
        this.sessionBus = dbus.sessionBus();
      } catch (err) {
        this.sessionBus = null;
      }

      if (!this.sessionBus) {
        consola.error("DBus: not connected to session bus");
        return;
      }
      this._register(dbus);
    }).catch((err) => {
      consola.warn("DBus: module not available (optional dependency)", err.message);
    });
  }

  _register(dbus) {
    this.sessionBus.requestName(SERVICE_NAME, 0x4, (err, retCode) => {

      if (err) {
        consola.error("Error DBus:", err);
        return;
      }

      if (retCode !== 1) {
        return;
      }

      this.sessionBus?.exportInterface(
        {
          Show: () => {
            if (this.window) windowShow(this.window);
          },
          Hide: () => {
            this.window?.hide();
          },
          Visible: () => {
            return this.window?.isVisible() ?? false;
          },
          ToggleVisibility: () => {
            if (this.window) return toggleVisibility(this.window);
            else return false;
          },
          Quit: () => {
            app.quit();
          },
        },
        OBJECT_PATH,
        {
          name: SERVICE_NAME,
          methods: {
            Show: ["", ""],
            Hide: ["", ""],
            ToggleVisibility: ["", "b"],
            Visible: ["", "b"],
            Quit: ["", ""],
          },
        },
      );

      consola.info("DBus registered:", SERVICE_NAME);
    });
  }

  setBadgeCount(number) {
    // https://wiki.ubuntu.com/Unity/LauncherAPI
    // gnome dash-to-dock: https://github.com/micheleg/dash-to-dock/blob/b96a003bec758fc35efb0f4e5134030a921c418b/launcherAPI.js#L14-L21
    // kde plasma: https://github.com/KDE/plasma-desktop/commit/e284e9dc17051f22d05985e218fa44ddaba78de5
    if (!this.sessionBus || !this.dbusRef) return;
    const hint = process.env.BAMF_DESKTOP_FILE_HINT;
    const snap = process.env.SNAP;
    const desktopFile = hint ? path.basename(hint) : snap ? "whatszan_whatszan.desktop" : "whatszan.desktop";
    this.sessionBus?.connection.message({
      type: this.dbusRef.messageType.signal,
      serial: ++this.serialCounter,
      path: "/",
      interface: "com.canonical.Unity.LauncherEntry",
      member: "Update",
      signature: "sa{sv}",
      body: [
        `application://${desktopFile}`,
        [
          ["count", ["x", number]],
          ["count-visible", ["b", number !== 0]],
        ],
      ],
    });
  }

  end() {
    this.sessionBus?.connection.end();
  }
}

