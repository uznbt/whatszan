import { app, dialog, Menu, MenuItem, globalShortcut } from "electron";
import { toggleVisibility, showToastInWindow, isWaylandSession } from "./util.mjs";
import { requestSessionRestore } from "./session-backup.mjs";

export const setupAppMenu = (mainWindow, openSettings, persistState, updateBlurState) => {
  const appMenu = Menu.getApplicationMenu();
  if (appMenu) {
    const windowMenu = appMenu.items.find(item => item.role === 'windowmenu' || item.label === 'Window' || item.label === 'Jendela');
    // We use a custom property to ensure we only setup once
    if (windowMenu && !windowMenu.customSetupDone) {
      windowMenu.customSetupDone = true;
      windowMenu.submenu.append(new MenuItem({ type: 'separator' }));
      
      windowMenu.submenu.append(new MenuItem({
        id: 'settings_appmenu',
        label: "Pengaturan...",
        accelerator: 'CmdOrCtrl+,',
        click: openSettings
      }));

      // ── Pemulihan sesi ────────────────────────────────────────────────────
      // Kredensial WhatsApp Web disimpan di IndexedDB. Kalau proses sempat
      // dimatikan paksa, database itu bisa rusak dan WhatsApp minta scan QR
      // lagi. Snapshot dibuat otomatis setiap kali aplikasi dijalankan,
      // dan bisa dipulihkan lewat menu ini.
      windowMenu.submenu.append(new MenuItem({ type: 'separator' }));
      windowMenu.submenu.append(new MenuItem({
        id: 'restore_session_appmenu',
        label: "Pulihkan Sesi WhatsApp dari Cadangan",
        click: () => {
          const parent = mainWindow && !mainWindow.isDestroyed() ? mainWindow : null;
          const show = (options) =>
            parent ? dialog.showMessageBoxSync(parent, options) : dialog.showMessageBoxSync(options);

          const result = requestSessionRestore();

          if (!result.ok) {
            show({
              type: 'info',
              title: 'Cadangan sesi',
              message: 'Belum ada cadangan sesi.',
              detail: 'Cadangan dibuat otomatis setiap kali WhatsZan dijalankan.',
            });
            return;
          }

          const choice = show({
            type: 'question',
            title: 'Pulihkan sesi WhatsApp',
            message: 'Pulihkan sesi dari cadangan?',
            detail:
              `Cadangan: ${result.snapshot}\n\n` +
              'WhatsZan akan ditutup lalu dibuka kembali secara otomatis.\n\n' +
              'Gunakan ini hanya kalau WhatsApp meminta scan QR padahal sebelumnya ' +
              'sudah login — biasanya setelah aplikasi tertutup paksa atau crash.',
            buttons: ['Pulihkan & Mulai Ulang', 'Batal'],
            defaultId: 1,
            cancelId: 1,
            noLink: true,
          });

          if (choice !== 0) return;

          app.relaunch();
          app.quit();
        }
      }));

      // ── Global shortcut ─────────────────────────────────────────────────────
      // CATATAN PENTING: globalShortcut Chromium di Linux hanya bekerja lewat
      // X11 (XGrabKey). Di Wayland, register() tetap sukses dan
      // isRegistered() juga true, TAPI shortcutionya tidak pernah terpicu -
      // sehingga Ctrl+Shift+B (Mode Rahasia) dan Ctrl+Shift+H (Anti Screen
      // Cast) diam-diam tidak berfungsi.
      //
      // Karena itu pada sesi Wayland shortcut-nya ditangani di sisi renderer
      // (src-web/injected.js), di mana tombol ditekan saat window WhatsApp
      // sedang aktif - persis skenario pemakaian fitur ini.
      if (!isWaylandSession()) {
        globalShortcut.register('CommandOrControl+Shift+B', () => {
          const current = persistState.get("privacy-blur", false);
          const newState = !current;
          persistState.set("privacy-blur", newState);
          updateBlurState(newState);
          showToastInWindow(mainWindow, newState ? 'Privacy Blur: ON' : 'Privacy Blur: OFF');
        });

        globalShortcut.register('CommandOrControl+Shift+H', () => {
          const current = persistState.get("anti-screencast", false);
          const newState = !current;
          persistState.set("anti-screencast", newState);
          if (mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.setContentProtection(newState);
            mainWindow.webContents.send("settings-saved");
          }
          showToastInWindow(mainWindow, newState ? 'Anti Screen Cast: ON' : 'Anti Screen Cast: OFF');
        });
      }
    }
  }

  // Ensure settings shortcut works even when menu bar is hidden
  if (mainWindow && !mainWindow.settingsShortcutHooked) {
    mainWindow.settingsShortcutHooked = true;
    mainWindow.webContents.on('before-input-event', (event, input) => {
      if ((input.control || input.meta || input.alt) && input.key.toLowerCase() === ',') {
        openSettings();
        event.preventDefault();
      }
    });
  }
};
