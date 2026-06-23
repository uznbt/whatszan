import { Menu, MenuItem, app, nativeImage, net } from "electron";
import { JsonConfig } from "./json-config.mjs";
import { readFileSync } from "node:fs";
import path from "node:path";
import { consola } from "consola";
import sharp from "sharp";

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

export function monitorSystemTheme(nativeTheme) {
  const checkTheme = () => {
    // 1. Freedesktop Portal (Modern GNOME, KDE, Hyprland via xdg-desktop-portal)
    exec('dbus-send --session --print-reply=literal --dest=org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.portal.Settings.Read string:"org.freedesktop.appearance" string:"color-scheme"', (err, stdout) => {
      if (!err && stdout) {
        if (stdout.includes('uint32 1')) { nativeTheme.themeSource = 'dark'; return; }
        if (stdout.includes('uint32 2')) { nativeTheme.themeSource = 'light'; return; }
        // If uint32 0 (No preference), fall through to the next checks
      }

      // 2. GNOME / Cinnamon / GTK (gsettings)
      exec('gsettings get org.gnome.desktop.interface color-scheme', (err2, stdout2) => {
        if (!err2 && stdout2) {
          if (stdout2.includes('prefer-dark')) { nativeTheme.themeSource = 'dark'; return; }
          if (stdout2.includes('prefer-light')) { nativeTheme.themeSource = 'light'; return; }
          // If 'default', fall through to the next checks
        }

        exec('gsettings get org.gnome.desktop.interface gtk-theme', (err3, stdout3) => {
          if (!err3 && stdout3 && stdout3.toLowerCase().includes('dark') && !stdout3.toLowerCase().includes('adw-gtk3')) { 
            nativeTheme.themeSource = 'dark'; 
            return; 
          }

          // 3. KDE Plasma / Qt
          exec('kreadconfig6 --group "Colors:Window" --key "BackgroundNormal" || kreadconfig5 --group "Colors:Window" --key "BackgroundNormal"', (err4, stdout4) => {
            if (!err4 && stdout4) {
              const parts = stdout4.trim().split(',');
              if (parts.length === 3) {
                const [r, g, b] = parts.map(Number);
                if (r + g + b < 384) { nativeTheme.themeSource = 'dark'; return; }
                else { nativeTheme.themeSource = 'light'; return; }
              }
            }
            
            // Absolute fallback
            nativeTheme.themeSource = 'system';
          });
        });
      });
    });
  };

  checkTheme();
  setInterval(checkTheme, 5000);
}
