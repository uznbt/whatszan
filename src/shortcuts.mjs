import { app, shell } from "electron";
import path from "node:path";
import { consola } from "consola";
import { loadTranslations } from "./util.mjs";

export const applyAutoRun = (autoRun) => {
  if (process.platform === 'win32') {
    import('fs').then(({ existsSync, unlinkSync }) => {
      const shortcutPath = path.join(process.env.APPDATA || '', 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup', 'WhatsZan.lnk');
      if (autoRun) {
        const exePath = app.getPath('exe');
        const workDir = path.dirname(exePath);
        const ps = `$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('${shortcutPath}'); $s.TargetPath = '${exePath}'; $s.Arguments = '--hide'; $s.WorkingDirectory = '${workDir}'; $s.Save()`;
        import('child_process').then(({ execSync }) => {
          try { execSync(`powershell -NoProfile -Command "${ps}"`, { stdio: 'ignore' }); } catch(e){}
        });
      } else {
        try { if (existsSync(shortcutPath)) unlinkSync(shortcutPath); } catch(e){}
      }
    });
  } else if (process.platform === 'linux') {
    import('fs').then(({ writeFileSync, unlinkSync, existsSync, mkdirSync }) => {
      const autostartDir = path.join(app.getPath('home'), '.config', 'autostart');
      const desktopFile = path.join(autostartDir, 'whatszan.desktop');
      try { if (!existsSync(autostartDir)) mkdirSync(autostartDir, { recursive: true }); } catch (e) {}

      if (autoRun) {
        const exePath = app.getPath('exe');
        const desktopEntry = `[Desktop Entry]\nType=Application\nExec="${exePath}" --hide\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\nName=WhatsZan\nComment=Start WhatsZan hidden on login`;
        try { writeFileSync(desktopFile, desktopEntry); } catch(e) { consola.error(e) }
      } else {
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
    import('fs').then(({ existsSync, unlinkSync, writeFileSync, mkdirSync }) => {
      const desktopPath = app.getPath('desktop');
      const applicationsPath = path.join(app.getPath('home'), '.local', 'share', 'applications');
      const oldDesktopShortcutPath = path.join(desktopPath, `${oldName || 'WhatsZan'}.desktop`);
      const newDesktopShortcutPath = path.join(desktopPath, `${newName || 'WhatsZan'}.desktop`);
      const appDesktopFile = path.join(applicationsPath, 'whatszan.desktop');
      
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
         
         // Remove Linux File Manager Context Menus
         const kioServices = path.join(app.getPath('home'), '.local', 'share', 'kio', 'servicemenus', 'whatszan-share.desktop');
         const kservices5 = path.join(app.getPath('home'), '.local', 'share', 'kservices5', 'ServiceMenus', 'whatszan-share.desktop');
         try { if (existsSync(kioServices)) unlinkSync(kioServices); } catch(e){}
         try { if (existsSync(kservices5)) unlinkSync(kservices5); } catch(e){}

         cleanLinuxScripts(newName || 'WhatsZan');
      }

      if (create) {
        const exePath = app.getPath('exe');
        const customAppIcon = persistState.get("custom_tray_app");
        const iconLocation = customAppIcon || 'whatszan';
        
        const desktopEntry = `[Desktop Entry]\nName=${newName || 'WhatsZan'}\nExec="${exePath}" %U\nTerminal=false\nType=Application\nIcon=${iconLocation}\nStartupWMClass=${newName || 'WhatsZan'}\nComment=WhatsZan Desktop Client\nCategories=Network;Chat;InstantMessaging;\nMimeType=x-scheme-handler/whatsapp;`;

        try {
          if (!existsSync(applicationsPath)) mkdirSync(applicationsPath, { recursive: true });
          writeFileSync(appDesktopFile, desktopEntry);
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
               execSync(`update-desktop-database "${applicationsPath}"`);
             } catch(e){}
          });
        } catch(e) {
          consola.error("Failed to create linux shortcut", e);
        }
      }
    });
  }
};
