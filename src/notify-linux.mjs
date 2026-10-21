import { app } from "electron";
import fs from "node:fs";
import path from "node:path";
import { consola } from "consola";
import { getSessionBus } from "./util.mjs";

// Notifikasi Linux yang bisa dibalas langsung dari shell.
//
// Electron tidak bisa mengirim custom hint ke org.freedesktop.Notifications,
// padahal hint itulah yang memberi tahu Quickshell (dan shell lain yang mau
// mendukung) notifikasi mana yang bisa dibalas dan ID apa yang harus dikirim
// balik. Jadi di Linux kita bicara langsung ke server notifikasi lewat
// koneksi D-Bus yang sudah dipakai aplikasi ini.
const DEST = "org.freedesktop.Notifications";
const OBJ = "/org/freedesktop/Notifications";
const IFACE = "org.freedesktop.Notifications";

let busPromise = null;
let serverPromise = null;

function getBus() {
  if (!busPromise) busPromise = getSessionBus();
  return busPromise;
}

async function invoke(member, signature, body) {
  const bus = await getBus();
  if (!bus) return null;
  return new Promise((resolve) => {
    const msg = { destination: DEST, path: OBJ, interface: IFACE, member };
    if (signature) {
      msg.signature = signature;
      msg.body = body;
    }
    try {
      bus.invoke(msg, (err, ...args) => {
        if (err) {
          consola.debug(`[notif] ${member} gagal: ${err.message}`);
          return resolve(null);
        }
        resolve(args);
      });
    } catch (err) {
      consola.debug(`[notif] ${member} error: ${err.message}`);
      resolve(null);
    }
  });
}

// Quickshell menamai server notifikasinya "quickshell". Shell lain tetap
// memakai jalur Electron Notification yang lama supaya klik notifikasi tidak
// kehilangan fungsinya.
export async function isQuickshellNotifications() {
  if (!serverPromise) {
    serverPromise = invoke("GetServerInformation", "", []).then((args) => {
      // Panggilan gagal (bus belum siap): jangan simpan hasil negatif,
      // supaya notifikasi berikutnya mencoba lagi.
      if (!args) {
        serverPromise = null;
        return false;
      }
      const name = String(args?.[0] ?? "").toLowerCase();
      return name.includes("quickshell");
    });
  }
  return serverPromise;
}

function saveIcon(dataUrl, id) {
  if (typeof dataUrl !== "string" || !dataUrl.startsWith("data:image/")) return "";
  const match = /^data:image\/([a-z0-9+.-]+);base64,([\s\S]+)$/i.exec(dataUrl);
  if (!match) return "";
  let ext = match[1].toLowerCase();
  if (ext === "jpeg") ext = "jpg";
  if (ext === "svg+xml") ext = "svg";
  try {
    const dir = path.join(app.getPath("userData"), "notif-icons");
    fs.mkdirSync(dir, { recursive: true });
    pruneIcons(dir);
    const file = path.join(dir, `${String(id).replace(/[^a-z0-9._-]/gi, "_")}.${ext}`);
    fs.writeFileSync(file, Buffer.from(match[2], "base64"));
    return file;
  } catch (err) {
    consola.debug("[notif] gagal menyimpan ikon:", err.message);
    return "";
  }
}

// Avatar kontak bisa berganti-ganti; simpan hanya yang masih baru supaya
// folder tidak tumbuh tanpa batas.
function pruneIcons(dir) {
  try {
    const now = Date.now();
    for (const name of fs.readdirSync(dir)) {
      const file = path.join(dir, name);
      const stat = fs.statSync(file);
      if (now - stat.mtimeMs > 24 * 60 * 60 * 1000) fs.unlinkSync(file);
    }
  } catch {
    /* abaikan */
  }
}

/**
 * Kirim notifikasi lewat server notifikasi Linux.
 * @returns {Promise<boolean>} true kalau notifikasi benar-benar terkirim.
 */
export async function showLinuxNotification({ id, title, body, icon, appName }) {
  if (!(await isQuickshellNotifications())) return false;

  const iconPath = saveIcon(icon, id);
  const hints = [
    // Dibaca oleh QML di shell: menandai notifikasi ini bisa dibalas dan
    // memberi tahu ID yang harus dikirim kembali saat pengguna menekan Enter.
    ["x-whatszan-id", ["s", String(id)]],
    ["desktop-entry", ["s", "whatszan"]],
    ["urgency", ["y", 1]],
  ];

  const result = await invoke("Notify", "susssasa{sv}i", [
    String(appName || "WhatsZan"),
    0,
    iconPath,
    String(title ?? ""),
    String(body ?? ""),
    [],
    hints,
    -1,
  ]);

  return Number.isInteger(result?.[0]);
}

export function resetLinuxNotifierForTest() {
  busPromise = null;
  serverPromise = null;
}
