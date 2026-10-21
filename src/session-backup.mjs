import { app } from "electron";
import fs from "node:fs";
import path from "node:path";
import { consola } from "consola";

/**
 * Cadangan sesi WhatsApp Web.
 *
 * Kredensial perangkat tertaut WhatsApp Web disimpan oleh Chromium di
 * IndexedDB / Local Storage / Cookies di dalam folder userData. Kalau proses
 * dimatikan paksa (SIGKILL, OOM killer) atau dua proses memakai profil yang
 * sama, LevelDB-nya bisa rusak - gejalanya WhatsApp minta scan QR lagi.
 *
 * Snapshot diambil saat startup, SEBELUM Chromium membuka database, jadi
 * isinya konsisten. Kalau suatu saat sesi rusak, snapshot terakhir bisa
 * dipulihkan lewat menu tanpa perlu scan QR ulang.
 */

const ITEMS = ["IndexedDB", "Local Storage", "Session Storage", "Cookies", "Network"];
const KEEP = 5;
const FLAG = "restore-session.flag";
const SNAP_RE = /^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}$/;

const userData = () => app.getPath("userData");
const backupRoot = () => path.join(userData(), "session-backup");
const flagPath = () => path.join(userData(), FLAG);

const listSnapshots = () => {
  const root = backupRoot();
  if (!fs.existsSync(root)) return [];
  return fs
    .readdirSync(root)
    .filter((name) => SNAP_RE.test(name))
    .sort();
};

const stamp = () => new Date().toISOString().slice(0, 19).replace(/[:]/g, "-");

export function backupSessionData() {
  const root = backupRoot();
  try {
    let copied = 0;
    const dest = path.join(root, stamp());
    for (const item of ITEMS) {
      const src = path.join(userData(), item);
      if (!fs.existsSync(src)) continue;
      fs.mkdirSync(dest, { recursive: true });
      fs.cpSync(src, path.join(dest, item), { recursive: true, force: true });
      copied++;
    }

    if (copied === 0) {
      fs.rmSync(dest, { recursive: true, force: true });
      return;
    }

    const snapshots = listSnapshots();
    for (const old of snapshots.slice(0, Math.max(0, snapshots.length - KEEP))) {
      fs.rmSync(path.join(root, old), { recursive: true, force: true });
    }

    consola.info(`Cadangan sesi dibuat: ${path.basename(dest)} (${copied} item)`);
  } catch (err) {
    consola.warn("Gagal membuat cadangan sesi:", err?.message || err);
  }
}

export function restoreSessionBackupIfRequested() {
  const flag = flagPath();
  if (!fs.existsSync(flag)) return false;

  const snapshots = listSnapshots();
  const latest = snapshots[snapshots.length - 1];
  if (!latest) {
    consola.warn("Permintaan pulihkan sesi, tetapi tidak ada cadangan.");
    try { fs.rmSync(flag, { force: true }); } catch { /* ignore */ }
    return false;
  }

  try {
    const srcDir = path.join(backupRoot(), latest);
    // Simpan dulu data yang sekarang (mungkin rusak) supaya tidak hilang.
    const broken = path.join(backupRoot(), `rusak-${stamp()}`);
    let moved = 0;

    for (const item of ITEMS) {
      const current = path.join(userData(), item);
      if (fs.existsSync(current)) {
        fs.mkdirSync(broken, { recursive: true });
        fs.renameSync(current, path.join(broken, item));
        moved++;
      }
    }

    for (const item of fs.readdirSync(srcDir)) {
      if (item.startsWith("rusak-")) continue;
      fs.cpSync(path.join(srcDir, item), path.join(userData(), item), {
        recursive: true,
        force: true,
      });
    }

    consola.info(`Sesi dipulihkan dari cadangan ${latest} (data lama dipindah ke ${path.basename(broken)}${moved ? "" : ", tidak ada"})`);
    return true;
  } catch (err) {
    consola.error("Gagal memulihkan sesi:", err?.message || err);
    return false;
  } finally {
    try { fs.rmSync(flag, { force: true }); } catch { /* ignore */ }
  }
}

/**
 * Minta pemulihan pada startup berikutnya. Dipanggil dari menu.
 */
export function requestSessionRestore() {
  const snapshots = listSnapshots();
  if (!snapshots.length) return { ok: false, reason: "belum ada cadangan" };

  try {
    fs.mkdirSync(userData(), { recursive: true });
    fs.writeFileSync(flagPath(), snapshots[snapshots.length - 1], "utf8");
    return { ok: true, snapshot: snapshots[snapshots.length - 1] };
  } catch (err) {
    return { ok: false, reason: err?.message || String(err) };
  }
}

export function latestSnapshotName() {
  const snapshots = listSnapshots();
  return snapshots[snapshots.length - 1] || null;
}
