import { Menu, MenuItem, app, nativeImage, net } from "electron";
import { JsonConfig } from "./json-config.mjs";
import { readFileSync } from "node:fs";
import path from "node:path";
import { consola } from "consola";

// sharp (libvips) adalah modul native yang besar: memuatnya saat startup
// memetakan ~50-70 MB pustaka ke memori main process dan memperlambat boot.
// sharp hanya dibutuhkan saat mengonversi favicon WebP atau membuat badge
// tray, jadi kita muat secara malas (lazy) saat pertama kali dipakai.
let sharpPromise = null;
function loadSharp() {
  if (!sharpPromise) {
    sharpPromise = import("sharp")
      .then((mod) => mod.default ?? mod)
      .catch((err) => {
        sharpPromise = null;
        throw err;
      });
  }
  return sharpPromise;
}

// --- Session D-Bus (shared) --------------------------------------------------
// Satu koneksi ke session bus dipakai bersama oleh fitur D-Bus WhatsZan dan
// deteksi tema, supaya tidak ada dua socket D-Bus di process yang sama.
let dbusBusPromise = null;
export function getSessionBus() {
  if (!dbusBusPromise) {
    dbusBusPromise = import("@homebridge/dbus-native")
      .then((mod) => {
        const dbus = mod.default ?? mod;
        const bus = dbus.sessionBus();
        // Simpan referensi modul supaya konsumen bisa memakai helper-nya
        // (mis. messageType untuk membuat sinyal D-Bus).
        if (bus) bus.__dbusModule = dbus;
        return bus;
      })
      .catch((err) => {
        consola.debug("dbus-native unavailable:", err.message);
        return null;
      });
  }
  return dbusBusPromise;
}

export const toggleVisibility = function (window) {
  consola.debug("toggleVisibility");
  const vis = window.isVisible();
  if (vis) {
    window.hide();
  } else {
    window.show();
  }
  return !vis;
};

export const isDebug = process?.env?.DEBUG == 1;

export const addAboutMenuItem = () => {
  let menu = Menu.getApplicationMenu();
  if (menu == null || menu.items.length == 0) {
    return;
  }
  const helpMenu = menu.items.slice(-1)[0];
  helpMenu?.submenu.append(new MenuItem({ role: "about" }));
};

export function windowShow(window) {
  if (window.isMinimized()) {
    window.restore();
  }
  window.show();
}

export async function loadUrl(url, protocols = ["file:", "https:"]) {
  let data = null;
  if (!protocols.includes(url.protocol)) {
    consola.error("unsupported protocol", url.protocol);
    return null;
  }
  if (url.protocol == "file:") {
    let path = url.pathname;
    if (url.hostname == "~") {
      path = app.getPath("home") + path;
    }
    consola.debug("load from file", path);
    data = readFileSync(path, "utf-8");
  } else if (url.protocol == "https:") {
    consola.debug("load from", url.href);
    const res = await net.fetch(url.href);
    if (res.ok) {
      data = await res.text();
    } else {
      consola.error("fetch", res.status, res.statusText);
    }
  }
  return data;
}

export function getUrl(url) {
  try {
    return new URL(url);
  } catch (err) {
    return null;
  }
}

export function replaceVariables(script) {
  return script.replace("${userData}", app.getPath("userData"));
}

const userIconCache = new Map();

export function getUserIcon(url, state) {
  url = url.replace(/v[0-9]+\//, "");
  const basename = path.basename(url);

  if (userIconCache.has(basename)) {
    return userIconCache.get(basename);
  }
  const dir = state.iconsDir;
  const filePath = path.join(dir, `${basename}.png`);
  consola.debug("favicon", basename, filePath);
  try {
    const buffer = readFileSync(filePath);
    const icon = nativeImage.createFromBuffer(buffer);
    userIconCache.set(basename, icon);
    return icon;
  } catch (e) {
    // File does not exist or cannot be read
  }
  userIconCache.set(basename, null);
  return null;
}

const iconCache = new Map();

export async function getIcon(url, state) {
  const userIcon = getUserIcon(url, state);
  if (userIcon) {
    return userIcon;
  }

  // larger images
  url = url.replace("/1x/", "/2x/");

  if (iconCache.has(url)) {
    return iconCache.get(url);
  }
  const re = /https:\/\/(static\.whatsapp\.net|web\.whatsapp\.com)\//;

  consola.debug("url", url);
  if (url && re.test(url)) {
    const r = await fetch(url, {
      headers: { "User-Agent": state.userAgent },
    });
    if (!r.ok) {
      consola.error("fetch", url, r.status, r.statusText);
      return null;
    }
    const ab = await r.arrayBuffer();
    let buffer = Buffer.from(ab);
    const isWebp =
      url.endsWith(".webp") ||
      r.headers.get("content-type")?.includes("image/webp") ||
      (buffer.length >= 12 && buffer.toString("ascii", 0, 4) === "RIFF" && buffer.toString("ascii", 8, 12) === "WEBP");
    if (isWebp) {
      consola.debug("convert webp", url);
      const sharp = await loadSharp();
      buffer = await sharp(buffer).png().toBuffer();
    }
    const image = nativeImage.createFromBuffer(buffer);
    consola.debug("nativeImage size", image.getSize());
    iconCache.set(url, image);
    return image;
  }
}

export function loadTranslations(locale) {
  let translations = {};
  try {
    const filename = locale.split("-")[0];
    const data = readFileSync(path.join(import.meta.dirname, "..", "locales", `${filename}.json`), "utf-8");
    translations = JSON.parse(data);
  } catch (err) {
    consola.warn("cannot load translations", locale);
  }
  return translations;
}

export function getUnreadCountFromFavicon(faviconUrl) {
  const match = faviconUrl.match(/https:\/\/web\.whatsapp\.com\/favicon\/1x\/f(\d+)\//);
  return match ? parseInt(match[1], 10) : 0;
}

export const urlScheme = "whatsapp";
export function convertWhatsAppUrl(url) {
  try {
    if (url.startsWith(`${urlScheme}:`)) {
      const whatsappUrl = new URL(url);
      let webUrl = `https://web.whatsapp.com/${whatsappUrl.hostname}${whatsappUrl.pathname}${whatsappUrl.search}${whatsappUrl.hash}`;
      if (whatsappUrl.hostname === 'chat') {
        webUrl = `https://web.whatsapp.com/accept${whatsappUrl.search}`;
      }
      consola.info("Converting whatsapp:// URL:", url, "->", webUrl);
      return webUrl;
    }

    const parsed = new URL(url);
    if (parsed.hostname === 'chat.whatsapp.com') {
      const code = parsed.pathname.replace(/^\/+/, '');
      return `https://web.whatsapp.com/accept?code=${code}`;
    } else if (parsed.hostname === 'wa.me') {
      const phone = parsed.pathname.replace(/^\/+/, '');
      const search = parsed.search ? `&${parsed.search.slice(1)}` : '';
      return `https://web.whatsapp.com/send?phone=${phone}${search}`;
    } else if (parsed.hostname === 'api.whatsapp.com' && parsed.pathname.startsWith('/send')) {
      return `https://web.whatsapp.com/send${parsed.search}`;
    }
    
    return null;
  } catch (err) {
    consola.error("Failed to convert whatsapp URL:", url, err);
    return null;
  }
}

export async function getBadgedTrayIcon(iconInput, unreadCount) {
  try {
    let buffer;
    if (typeof iconInput === 'string') {
      buffer = readFileSync(iconInput);
    } else if (iconInput && iconInput.toPNG) {
      buffer = iconInput.toPNG();
    } else {
      return null;
    }

    if (unreadCount > 0) {
      // Create a red dot SVG
      const badgeSvg = Buffer.from(
        `<svg width="120" height="120">
           <circle cx="60" cy="60" r="50" fill="#FF0000" stroke="#FFFFFF" stroke-width="10"/>
         </svg>`
      );
      const sharp = await loadSharp();
      buffer = await sharp(buffer)
        .composite([{ input: badgeSvg, top: 90, left: 300 }])
        .png()
        .toBuffer();
    }
    return nativeImage.createFromBuffer(buffer);
  } catch (err) {
    consola.error("Error creating badged tray icon", err);
    return typeof iconInput === 'string' ? nativeImage.createFromPath(iconInput) : iconInput;
  }
}

import { exec } from 'child_process';
import * as os from 'node:os';

export function monitorSystemTheme(nativeTheme) {
  // Windows & macOS: Electron dapat membaca tema sistem secara native.
  // Cukup set ke 'system' dan Electron akan mengikuti preferensi OS secara otomatis.
  if (os.platform() !== 'linux') {
    nativeTheme.themeSource = 'system';
    return;
  }

  // ── Linux ──────────────────────────────────────────────────────────────────
  // Deteksi manual karena Electron tidak selalu membaca tema Linux dengan
  // benar di semua DE.
  //
  // Versi lama memanggil `dbus-send`/`gsettings`/`kreadconfig` lewat exec()
  // setiap 5 detik. Itu berarti ~17.000 fork+exec per hari: setiap fork menyalin
  // page table main process (~300 MB) dan membangunkan CPU dari deep C-state,
  // jadi laptop tidak bisa tidur sejenak.
  //
  // Sekarang pembacaan dilakukan IN-PROCESS lewat dbus-native (bus yang sama
  // dengan yang dipakai fitur D-Bus WhatsZan) sehingga tidak ada fork sama
  // sekali. Subprocess hanya dipakai sebagai cadangan kalau dbus-native tidak
  // tersedia, dan itupun dengan jeda jauh lebih panjang.
  //
  // Nilai tema juga di-cache: `themeSource` hanya di-set ketika benar-benar
  // berubah, sehingga tidak memicu repaint seluruh window tiap 5 detik.

  let lastAppliedTheme = null;
  const applyTheme = (theme) => {
    if (!theme || theme === lastAppliedTheme) return;
    lastAppliedTheme = theme;
    nativeTheme.themeSource = theme;
    consola.debug("system theme ->", theme);
  };

  // --- Pembacaan via D-Bus in-process ---------------------------------------
  // Proxy interface di-cache. `getObject()` melakukan Introspect penuh
  // (parsing XML ~20 interface, butuh ~30 ms!) - kalau dilakukan tiap poll
  // justru lebih lambat daripada memanggil `dbus-send`. Jadi sekali saja,
  // lalu proxy-nya dipakai ulang.
  let settingsProxy = null;
  const getSettingsProxy = async () => {
    if (settingsProxy) return settingsProxy;
    const bus = await getSessionBus();
    if (!bus) return null;
    try {
      const obj = await new Promise((resolve, reject) => {
        bus.getObject("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", (err, res) => {
          if (err) reject(err);
          else resolve(res);
        });
      });
      const iface = obj?.proxy?.["org.freedesktop.portal.Settings"];
      if (typeof iface?.["Read"] !== "function") return null;
      settingsProxy = iface;
      return settingsProxy;
    } catch (err) {
      consola.debug("portal introspect failed:", err.message);
      return null;
    }
  };

  // Portal Settings.Read → uint32: 0 = no preference, 1 = dark, 2 = light
  const readThemeViaDBus = async () => {
    const iface = await getSettingsProxy();
    if (!iface) return null;
    try {
      const raw = await new Promise((resolve, reject) => {
        iface["Read"].call(iface, "org.freedesktop.appearance", "color-scheme", (err, res) => {
          if (err) reject(err);
          else resolve(res);
        });
      });
      // dbus-native mengembalikan body marshalled: cari nilai uint32 di dalamnya
      const value = extractUint32(raw);
      if (value === 1) return "dark";
      if (value === 2) return "light";
    } catch (err) {
      consola.debug("portal theme read failed:", err.message);
      // Proxy bisa jadi basi kalau portal restart; buang cache agar
      // introspect dilakukan ulang pada poll berikutnya.
      settingsProxy = null;
    }
    return null;
  };

  // Fallback subprocess (hanya bila D-Bus in-process tidak tersedia)
  const readThemeViaSubprocess = () => {
    return new Promise((resolve) => {
      exec('dbus-send --session --print-reply=literal --dest=org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.portal.Settings.Read string:"org.freedesktop.appearance" string:"color-scheme"', (err, stdout) => {
        if (!err && stdout) {
          if (stdout.includes("uint32 1")) return resolve("dark");
          if (stdout.includes("uint32 2")) return resolve("light");
        }
        resolve(null);
      });
    });
  };

  // --- Deteksi fallback (gsettings / kreadconfig) ----------------------------
  const readThemeViaGtk = () => {
    return new Promise((resolve) => {
      exec('gsettings get org.gnome.desktop.interface color-scheme', (err, stdout) => {
        if (!err && stdout) {
          if (stdout.includes("prefer-dark")) return resolve("dark");
          if (stdout.includes("prefer-light")) return resolve("light");
        }
        exec('gsettings get org.gnome.desktop.interface gtk-theme', (err2, stdout2) => {
          if (!err2 && stdout2 && stdout2.toLowerCase().includes("dark") && !stdout2.toLowerCase().includes("adw-gtk3")) {
            return resolve("dark");
          }
          // 3. KDE Plasma / Qt
          exec('kreadconfig6 --group "Colors:Window" --key "BackgroundNormal" || kreadconfig5 --group "Colors:Window" --key "BackgroundNormal"', (err3, stdout3) => {
            if (!err3 && stdout3) {
              const parts = stdout3.trim().split(",");
              if (parts.length === 3) {
                const [r, g, b] = parts.map(Number);
                return resolve(r + g + b < 384 ? "dark" : "light");
              }
            }
            resolve(null);
          });
        });
      });
    });
  };

  let checking = false;
  const checkTheme = async () => {
    if (checking) return;
    checking = true;
    try {
      let theme = await readThemeViaDBus();
      if (!theme) theme = await readThemeViaSubprocess();
      if (!theme) theme = await readThemeViaGtk();
      applyTheme(theme);
    } catch (err) {
      consola.debug("checkTheme failed", err.message);
    } finally {
      checking = false;
    }
  };

  checkTheme();
  setInterval(checkTheme, 5000);
}

// Cari nilai uint32 di dalam body marshalled dbus-native.
// Bentuknya kira-kira: [[{"type":"v","child":[]}],[[[{"type":"u","child":[]}],[1]]]]
function extractUint32(raw) {
  const seen = new Set();
  const walk = (node) => {
    if (node === null || node === undefined) return null;
    if (typeof node === "number") return node;
    if (typeof node !== "object") return null;
    if (seen.has(node)) return null;
    seen.add(node);
    if (Array.isArray(node)) {
      for (const item of node) {
        const found = walk(item);
        if (found !== null) return found;
      }
      return null;
    }
    if ("value" in node) {
      const found = walk(node.value);
      if (found !== null) return found;
    }
    if ("child" in node) {
      const found = walk(node.child);
      if (found !== null) return found;
    }
    return null;
  };
  return walk(raw);
}
