import { app, shell } from "electron";
import path from "node:path";
import { existsSync } from "node:fs";
import { consola } from "consola";
import { loadTranslations } from "./util.mjs";

/**
 * Path executable yang stabil untuk dipakai di file .desktop.
 *
 * `app.getPath('exe')` menunjuk ke folder mount sementara saat aplikasi
 * dijalankan dari AppImage (/tmp/appimage_extracted_XXXX/whatszan). Folder itu
 * dihapus begitu AppImage ditutup, sehingga shortcut yang dibuat menjadi rusak
 * (icon masih ada tapi diklik tidak melakukan apa-apa). Env APPIMAGE menunjuk
 * ke file .AppImage itu sendiri, yang lokasinya stabil.
 */
export const getStableExePath = () => process.env.APPIMAGE || app.getPath('exe');

/**
 * Nama file .desktop milik pengguna.
 *
 * WAJIB sama persis dengan file yang dipasang electron-builder
 * (/usr/share/applications/WhatsZan.desktop). Kalau berbeda, Desktop
 * Environment akan menampilkan DUA entri WhatsZan di launcher: satu dari
 * paket sistem dan satu dari ~/.local/share/applications. File dengan nama
 * yang sama di ~/.local/share/applications menimpa yang sistem, jadi hanya
 * satu entri yang muncul - sekaligus membuat ikon kustom bisa bekerja.
 */
const USER_DESKTOP_FILENAME = 'WhatsZan.desktop';

// Nama file versi lama (huruf kecil) - dibersihkan saat migrasi.
const LEGACY_DESKTOP_FILENAME = 'whatszan.desktop';

/**
 * Cek apakah baris Exec yang sudah ada masih bisa dipakai.
 * Path absolut yang sudah hilang (mis. folder mount AppImage yang sudah
 * dihapus) tidak bisa dipakai - dalam kasus itu baris Exec diganti dengan
 * milik aplikasi.
 */
const isExecUsable = (execLine) => {
  if (!execLine) return false;
  const m = execLine.match(/Exec="?([^"\s]+)"?/);
  if (!m) return false;
  const target = m[1];
  if (!target.startsWith('/')) return true; // perintah yang ada di PATH
  try {
    return existsSync(target);
  } catch (e) {
    return false;
  }
};

/**
 * Pastikan entri autostart tidak muncul di launcher.
 * Hanya NoDisplay/Hidden yang diubah, Exec milik user tetap utuh.
 */
const hideFromLauncher = (content) => {
  let out = content;
  if (/^NoDisplay=/m.test(out)) {
    out = out.replace(/^NoDisplay=.*$/m, 'NoDisplay=true');
  } else {
    out = out.trimEnd() + '\nNoDisplay=true\n';
  }
  if (/^Hidden=/m.test(out)) {
    out = out.replace(/^Hidden=.*$/m, 'Hidden=true');
  } else {
    out = out.trimEnd() + '\nHidden=true\n';
  }
  return out;
};

export const applyAutoRun = (autoRun, startupRun = false) => {
  if (process.platform === 'win32') {
    import('fs').then(({ existsSync, unlinkSync }) => {
      const shortcutPath = path.join(process.env.APPDATA || '', 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup', 'WhatsZan.lnk');
      if (autoRun) {
        if (!force && existsSync(shortcutPath)) return;
        const exePath = app.getPath('exe');
        const workDir = path.dirname(exePath);
        try {
          shell.writeShortcutLink(shortcutPath, 'create', {
            target: exePath,
            args: '--hide',
            cwd: workDir,
            description: 'Start WhatsZan hidden on login'
          });
        } catch (e) {
          consola.error("Failed to create auto-run shortcut", e);
        }
      } else {
        try { if (existsSync(shortcutPath)) unlinkSync(shortcutPath); } catch(e){}
      }
    });
  } else if (process.platform === 'linux') {
    import('fs').then(({ writeFileSync, unlinkSync, existsSync, mkdirSync, readFileSync }) => {
      const autostartDir = path.join(app.getPath('home'), '.config', 'autostart');
      const desktopFile = path.join(autostartDir, USER_DESKTOP_FILENAME);
      const legacyFile = path.join(autostartDir, LEGACY_DESKTOP_FILENAME);
      try { if (!existsSync(autostartDir)) mkdirSync(autostartDir, { recursive: true }); } catch (e) {}

      // Bersihkan file versi lama (huruf kecil) supaya tidak ada dua entri
      // autostart yang memicu aplikasi dua kali.
      try { if (existsSync(legacyFile)) unlinkSync(legacyFile); } catch(e) {}

      // File autostart WAJIB disembunyikan dari launcher, apa pun isinya.
      // Entri autostart bukan shortcut aplikasi - kalau tampil di launcher,
      // user melihat dua ikon WhatsZan.
      if (existsSync(desktopFile)) {
        try {
          const current = readFileSync(desktopFile, 'utf-8');
          const updated = hideFromLauncher(current);
          if (updated !== current) writeFileSync(desktopFile, updated);
        } catch(e) {}
      }

      if (autoRun) {
        if (!existsSync(desktopFile)) {
          const exePath = getStableExePath();
          const desktopEntry = `[Desktop Entry]\nType=Application\nExec="${exePath}" --hide\nNoDisplay=true\nHidden=true\nX-GNOME-Autostart-enabled=true\nName=WhatsZan\nComment=Start WhatsZan hidden on login`;
          try { writeFileSync(desktopFile, desktopEntry); } catch(e) { consola.error(e) }
        }
      } else if (!startupRun) {
        // File HANYA dihapus saat user mematikan auto-run lewat pengaturan.
        // Saat startup/update aplikasi tidak boleh menghapus file milik user
        // yang tidak dibuat oleh aplikasi ini.
        try { if (existsSync(desktopFile)) unlinkSync(desktopFile); } catch(e) {}
      }
    });
  } else {
    app.setLoginItemSettings({ openAtLogin: autoRun, args: autoRun ? ['--hide'] : [] });
  }
};

export const applyDesktopShortcut = (create, oldName, newName, config, persistState) => {
  if (process.platform === 'win32') {
    import('fs').then(({ existsSync, unlinkSync }) => {
      const desktopPath = app.getPath('desktop');
      const startMenuPath = path.join(app.getPath('appData'), 'Microsoft', 'Windows', 'Start Menu', 'Programs');
      const sendToPath = path.join(app.getPath('appData'), 'Microsoft', 'Windows', 'SendTo');
      
      const appLang = config.get("app-language", "auto");
      const lang = appLang !== "auto" ? appLang : app.getLocale();
      const translations = loadTranslations(lang);
      const tMedia = translations.share_media || "Bagikan Media";
      const tDoc = translations.share_document || "Bagikan Dokumen";
      
      const oldShortcutPath = path.join(desktopPath, `${oldName || 'WhatsZan'}.lnk`);
      const newShortcutPath = path.join(desktopPath, `${newName || 'WhatsZan'}.lnk`);
      const oldStartMenuShortcutPath = path.join(startMenuPath, `${oldName || 'WhatsZan'}.lnk`);
      const newStartMenuShortcutPath = path.join(startMenuPath, `${newName || 'WhatsZan'}.lnk`);
      const oldSendToShortcutPath = path.join(sendToPath, `${oldName || 'WhatsZan'}.lnk`);
      const newSendToShortcutPath = path.join(sendToPath, `${newName || 'WhatsZan'}.lnk`);
      const oldSendToMediaShortcutPath = path.join(sendToPath, `${oldName || 'WhatsZan'} (${tMedia}).lnk`);
      const newSendToMediaShortcutPath = path.join(sendToPath, `${newName || 'WhatsZan'} (${tMedia}).lnk`);
      const oldSendToDocShortcutPath = path.join(sendToPath, `${oldName || 'WhatsZan'} (${tDoc}).lnk`);
      const newSendToDocShortcutPath = path.join(sendToPath, `${newName || 'WhatsZan'} (${tDoc}).lnk`);
      
      try { if (existsSync(oldShortcutPath)) unlinkSync(oldShortcutPath); } catch(e){}
      try { if (existsSync(oldStartMenuShortcutPath)) unlinkSync(oldStartMenuShortcutPath); } catch(e){}
      try { if (existsSync(oldSendToShortcutPath)) unlinkSync(oldSendToShortcutPath); } catch(e){}
      try { if (existsSync(newSendToShortcutPath)) unlinkSync(newSendToShortcutPath); } catch(e){}
      try { if (existsSync(oldSendToMediaShortcutPath)) unlinkSync(oldSendToMediaShortcutPath); } catch(e){}
      try { if (existsSync(oldSendToDocShortcutPath)) unlinkSync(oldSendToDocShortcutPath); } catch(e){}
      
      if (newName && newName !== 'WhatsZan') {
        const defaultDesktopShortcut = path.join(desktopPath, 'WhatsZan.lnk');
        const defaultStartMenuShortcut = path.join(startMenuPath, 'WhatsZan.lnk');
        const defaultSendToShortcut = path.join(sendToPath, 'WhatsZan.lnk');
        const defaultSendToMediaShortcut = path.join(sendToPath, `WhatsZan (${tMedia}).lnk`);
        const defaultSendToDocShortcut = path.join(sendToPath, `WhatsZan (${tDoc}).lnk`);
        try { if (existsSync(defaultDesktopShortcut)) unlinkSync(defaultDesktopShortcut); } catch(e){}
        try { if (existsSync(defaultStartMenuShortcut)) unlinkSync(defaultStartMenuShortcut); } catch(e){}
        try { if (existsSync(defaultSendToShortcut)) unlinkSync(defaultSendToShortcut); } catch(e){}
        try { if (existsSync(defaultSendToMediaShortcut)) unlinkSync(defaultSendToMediaShortcut); } catch(e){}
        try { if (existsSync(defaultSendToDocShortcut)) unlinkSync(defaultSendToDocShortcut); } catch(e){}
      }
      
      if (!create) {
         try { if (existsSync(newShortcutPath)) unlinkSync(newShortcutPath); } catch(e){}
         try { if (existsSync(newStartMenuShortcutPath)) unlinkSync(newStartMenuShortcutPath); } catch(e){}
         try { if (existsSync(newSendToMediaShortcutPath)) unlinkSync(newSendToMediaShortcutPath); } catch(e){}
         try { if (existsSync(newSendToDocShortcutPath)) unlinkSync(newSendToDocShortcutPath); } catch(e){}
      }

      if (create) {
        const exePath = app.getPath('exe');
        const workDir = path.dirname(exePath);
        const customAppIcon = persistState.get("custom_tray_app");
        const iconLocation = customAppIcon || exePath;
        const shortcutOptions = {
          target: exePath,
          cwd: workDir,
          icon: iconLocation,
          iconIndex: 0,
          appUserModelId: 'org.uznbt.whatszan',
          description: 'WhatsZan Desktop Client'
        };
        try {
          shell.writeShortcutLink(newShortcutPath, 'create', shortcutOptions);
          shell.writeShortcutLink(newStartMenuShortcutPath, 'create', shortcutOptions);
          shell.writeShortcutLink(newSendToMediaShortcutPath, 'create', { ...shortcutOptions, args: '--share-media' });
          shell.writeShortcutLink(newSendToDocShortcutPath, 'create', { ...shortcutOptions, args: '--share-document' });
        } catch(e) {
          consola.error("Failed to create shortcut using shell API", e);
        }
      }
    });
  } else if (process.platform === 'linux') {
    import('fs').then(({ existsSync, unlinkSync, writeFileSync, mkdirSync, readFileSync }) => {
      const desktopPath = app.getPath('desktop');
      const applicationsPath = path.join(app.getPath('home'), '.local', 'share', 'applications');
      const oldDesktopShortcutPath = path.join(desktopPath, `${oldName || 'WhatsZan'}.desktop`);
      const newDesktopShortcutPath = path.join(desktopPath, `${newName || 'WhatsZan'}.desktop`);
      const appDesktopFile = path.join(applicationsPath, USER_DESKTOP_FILENAME);
      const legacyAppDesktopFile = path.join(applicationsPath, LEGACY_DESKTOP_FILENAME);
      
      const scriptDirs = [
        path.join(app.getPath('home'), '.local', 'share', 'nautilus', 'scripts'),
        path.join(app.getPath('home'), '.local', 'share', 'nemo', 'scripts'),
        path.join(app.getPath('home'), '.config', 'caja', 'scripts')
      ];

      const cleanLinuxScripts = (nameToClean) => {
        scriptDirs.forEach(dir => {
          import('fs').then(({ readdirSync, lstatSync }) => {
             try {
               if (existsSync(dir)) {
                 readdirSync(dir).forEach(file => {
                   if (file.includes(nameToClean)) {
                     const fp = path.join(dir, file);
                     if (lstatSync(fp).isFile()) unlinkSync(fp);
                   }
                 });
               }
             } catch(e) {}
          });
        });
      };

      try { if (existsSync(oldDesktopShortcutPath)) unlinkSync(oldDesktopShortcutPath); } catch(e){}
      cleanLinuxScripts(oldName || 'WhatsZan');
      
      if (newName && newName !== 'WhatsZan') {
        const defaultDesktopShortcut = path.join(desktopPath, 'WhatsZan.desktop');
        try { if (existsSync(defaultDesktopShortcut)) unlinkSync(defaultDesktopShortcut); } catch(e){}
        cleanLinuxScripts('WhatsZan');
      }
      
      if (!create) {
         try { if (existsSync(newDesktopShortcutPath)) unlinkSync(newDesktopShortcutPath); } catch(e){}
         // Versi lama (huruf kecil) - hapus supaya tidak ada entri ganda.
         try { if (existsSync(legacyAppDesktopFile)) unlinkSync(legacyAppDesktopFile); } catch(e){}
         
         // Remove Linux File Manager Context Menus
         const kioServices = path.join(app.getPath('home'), '.local', 'share', 'kio', 'servicemenus', 'whatszan-share.desktop');
         const kservices5 = path.join(app.getPath('home'), '.local', 'share', 'kservices5', 'ServiceMenus', 'whatszan-share.desktop');
         try { if (existsSync(kioServices)) unlinkSync(kioServices); } catch(e){}
         try { if (existsSync(kservices5)) unlinkSync(kservices5); } catch(e){}

         cleanLinuxScripts(newName || 'WhatsZan');
      }

      const exePath = getStableExePath();
      // Pilihan ikon bawaan memakai NAMA ikon (di search path icon theme),
      // sedangkan ikon kustom memakai path absolut ke file gambarnya.
      const iconChoice = persistState.get("icon-choice-app", "whatszan");
      const customAppIcon = persistState.get("custom_tray_app");
      const iconLocation =
        (iconChoice === 'whatszan' || iconChoice === 'whatsapp') ? iconChoice : (customAppIcon || 'whatszan');

      const desktopEntry = `[Desktop Entry]\nName=${newName || 'WhatsZan'}\nExec="${exePath}" %U\nTerminal=false\nType=Application\nIcon=${iconLocation}\nStartupWMClass=${newName || 'WhatsZan'}\nComment=WhatsZan Desktop Client\nCategories=Network;Chat;InstantMessaging;\nMimeType=x-scheme-handler/whatsapp;`;

      // Kalau user sudah punya entri sendiri (mis. dengan wrapper launcher
      // custom), baris Exec-nya dipertahankan selama path-nya masih valid.
      // Nama dan ikon tetap pakai milik kita supaya branding bisa berubah.
      let finalEntry = desktopEntry;
      if (existsSync(appDesktopFile)) {
        try {
          const current = readFileSync(appDesktopFile, 'utf-8');
          const existingExec = current.match(/^Exec=.*$/m)?.[0];
          if (isExecUsable(existingExec)) {
            finalEntry = desktopEntry.replace(/^Exec=.*$/m, existingExec);
          }
        } catch(e) {}
      }

      try {
        if (!existsSync(applicationsPath)) mkdirSync(applicationsPath, { recursive: true });

        // Entri launcher SELALU dikelola, terpisah dari toggle "shortcut di
        // Desktop". Nama filenya sama persis dengan file dari paket
        // (/usr/share/applications/WhatsZan.desktop) sehingga yang di
        // ~/.local/share/applications menimpanya - hanya satu entri WhatsZan
        // yang muncul di launcher, dan ikon kustom bisa bekerja.
        writeFileSync(appDesktopFile, finalEntry);

        // Bersihkan file versi lama (huruf kecil). Kalau tidak dihapus,
        // launcher menampilkan dua entri WhatsZan.
        try { if (existsSync(legacyAppDesktopFile)) unlinkSync(legacyAppDesktopFile); } catch(e) {}
      } catch(e) {
        consola.error("Failed to write linux application entry", e);
      }

      if (create) {
        try {
          writeFileSync(newDesktopShortcutPath, desktopEntry);

          // --- Linux File Manager Context Menus ---
          const appLang = config.get("app-language", "auto");
          const lang = appLang !== "auto" ? appLang : app.getLocale();
          const translations = loadTranslations(lang);
          const tMedia = translations.share_media || "Bagikan Media";
          const tDoc = translations.share_document || "Bagikan Dokumen";

          const kioServices = path.join(app.getPath('home'), '.local', 'share', 'kio', 'servicemenus');
          const kservices5 = path.join(app.getPath('home'), '.local', 'share', 'kservices5', 'ServiceMenus');
          const dolphinEntry = `[Desktop Entry]\nType=Service\nServiceTypes=KonqPopupMenu/Plugin\nMimeType=all/all;\nActions=ShareDocument;ShareMedia;\nX-KDE-Priority=TopLevel\n\n[Desktop Action ShareDocument]\nName=${newName || 'WhatsZan'} (${tDoc})\nIcon=${iconLocation}\nExec="${exePath}" --share-document %F\n\n[Desktop Action ShareMedia]\nName=${newName || 'WhatsZan'} (${tMedia})\nIcon=${iconLocation}\nExec="${exePath}" --share-media %F\n`;

          [kioServices, kservices5].forEach(dir => {
            try {
              if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
              writeFileSync(path.join(dir, 'whatszan-share.desktop'), dolphinEntry);
            } catch(e) {}
          });

          const scriptDoc = `#!/bin/bash\n"${exePath}" --share-document "$@"\n`;
          const scriptMedia = `#!/bin/bash\n"${exePath}" --share-media "$@"\n`;

          scriptDirs.forEach(dir => {
            try {
              if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
              const docPath = path.join(dir, `${newName || 'WhatsZan'} (${tDoc})`);
              const mediaPath = path.join(dir, `${newName || 'WhatsZan'} (${tMedia})`);
              writeFileSync(docPath, scriptDoc);
              writeFileSync(mediaPath, scriptMedia);
              import('child_process').then(({ execSync }) => {
                 try {
                   execSync(`chmod +x "${docPath}"`);
                   execSync(`chmod +x "${mediaPath}"`);
                 } catch(e){}
              });
            } catch(e) {}
          });
          // --- End Linux File Manager Context Menus ---

          import('child_process').then(({ execSync }) => {
             try {
               execSync(`chmod +x "${newDesktopShortcutPath}"`);
             } catch(e){}
          });
        } catch(e) {
          consola.error("Failed to create linux shortcut", e);
        }
      }

      // Segarkan cache desktop database supaya launcher langsung memakai
      // entri terbaru (nama, ikon, dan Exec).
      import('child_process').then(({ execSync }) => {
        try {
          execSync(`update-desktop-database "${applicationsPath}"`);
        } catch(e){}
      });
    });
  }
};
