import { Menu, MenuItem, globalShortcut } from "electron";

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

      // Setup global shortcuts for features
      const showToast = (msg) => {
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.webContents.executeJavaScript(`
            (function() {
              let t = document.getElementById('wz-toast');
              if (!t) {
                t = document.createElement('div');
                t.id = 'wz-toast';
                Object.assign(t.style, {
                  position: 'fixed', top: '24px', left: '50%', transform: 'translateX(-50%)',
                  backgroundColor: 'rgba(0, 0, 0, 0.85)', color: '#fff', padding: '10px 20px',
                  borderRadius: '20px', zIndex: '999999', fontSize: '14px', fontWeight: '500',
                  transition: 'opacity 0.3s ease', opacity: '0', pointerEvents: 'none',
                  boxShadow: '0 4px 12px rgba(0,0,0,0.3)'
                });
                document.body.appendChild(t);
              }
              t.innerText = ${JSON.stringify(msg)};
              t.style.opacity = '1';
              if (t.timer) clearTimeout(t.timer);
              t.timer = setTimeout(() => t.style.opacity = '0', 2500);
            })();
          `).catch(()=>{});
        }
      };

      globalShortcut.register('CommandOrControl+Shift+B', () => {
        const current = persistState.get("privacy-blur", false);
        const newState = !current;
        persistState.set("privacy-blur", newState);
        updateBlurState(newState);
        showToast(newState ? 'Privacy Blur: ON' : 'Privacy Blur: OFF');
      });

      globalShortcut.register('CommandOrControl+Shift+H', () => {
        const current = persistState.get("anti-screencast", false);
        const newState = !current;
        persistState.set("anti-screencast", newState);
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.setContentProtection(newState);
          mainWindow.webContents.send("settings-saved");
        }
        showToast(newState ? 'Anti Screen Cast: ON' : 'Anti Screen Cast: OFF');
      });
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
