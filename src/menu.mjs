import { Menu, MenuItem, globalShortcut } from "electron";
import { toggleVisibility, showToastInWindow, isWaylandSession } from "./util.mjs";

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
