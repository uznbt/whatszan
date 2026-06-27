; installer.nsh — Custom NSIS hooks for WhatsZan
; Added by electron-builder via the "include" option

!include "WordFunc.nsh"

LangString MsgUninstData 1033 "Do you also want to DELETE ALL USER DATA and WhatsApp sessions?$\n$\n(Select 'No' if you plan to reinstall WhatsZan later)"
LangString MsgUninstData 1057 "Apakah kamu juga ingin MENGHAPUS SELURUH DATA PENGGUNA dan sesi WhatsApp?$\n$\n(Pilih 'No' jika kamu berencana untuk meng-install ulang WhatsZan nanti)"
LangString MsgUninstData 1031 "Möchten Sie auch ALLE BENUTZERDATEN und WhatsApp-Sitzungen LÖSCHEN?$\n$\n(Wählen Sie 'Nein', wenn Sie WhatsZan später neu installieren möchten)"
LangString MsgUninstData 1036 "Voulez-vous également SUPPRIMER TOUTES LES DONNÉES UTILISATEUR et les sessions WhatsApp ?$\n$\n(Sélectionnez 'Non' si vous prévoyez de réinstaller WhatsZan plus tard)"
LangString MsgUninstData 1040 "Vuoi anche ELIMINARE TUTTI I DATI UTENTE e le sessioni di WhatsApp?$\n$\n(Seleziona 'No' se prevedi di reinstallare WhatsZan in seguito)"
LangString MsgUninstData 1046 "Deseja também EXCLUIR TODOS OS DADOS DO USUÁRIO e sessões do WhatsApp?$\n$\n(Selecione 'Não' se você planeja reinstalar o WhatsZan mais tarde)"

LangString MsgUpgrade 1033 "Old version ($0) detected.$\n$\nDo you want to UPGRADE to the latest version (${VERSION})?$\n$\n- Click 'Yes' to UPGRADE.$\n- Click 'Cancel' to UNINSTALL.$\n- Click 'No' to Cancel."
LangString MsgUpgrade 1057 "WhatsZan versi lama ($0) terdeteksi.$\n$\nApakah kamu ingin melakukan UPGRADE ke versi terbaru (${VERSION})?$\n$\n- Klik 'Yes' untuk UPGRADE.$\n- Klik 'Cancel' untuk UNINSTALL.$\n- Klik 'No' untuk Batal."
LangString MsgUpgrade 1031 "Alte Version ($0) erkannt.$\n$\nMöchten Sie auf die neueste Version (${VERSION}) UPGRADEN?$\n$\n- Klicken Sie 'Ja' zum UPGRADE.$\n- Klicken Sie 'Abbrechen' zum DEINSTALLIEREN.$\n- Klicken Sie 'Nein' zum Abbrechen."
LangString MsgUpgrade 1036 "Ancienne version ($0) détectée.$\n$\nVoulez-vous effectuer une MISE À JOUR vers la dernière version (${VERSION}) ?$\n$\n- Cliquez 'Oui' pour METTRE À JOUR.$\n- Cliquez 'Annuler' pour DÉSINSTALLER.$\n- Cliquez 'Non' pour Annuler."
LangString MsgUpgrade 1040 "Versione precedente ($0) rilevata.$\n$\nVuoi fare l'UPGRADE all'ultima versione (${VERSION})?$\n$\n- Clicca 'Sì' per UPGRADE.$\n- Clicca 'Annulla' per DISINSTALLARE.$\n- Clicca 'No' per Annullare."
LangString MsgUpgrade 1046 "Versão antiga ($0) detectada.$\n$\nDeseja fazer o UPGRADE para a versão mais recente (${VERSION})?$\n$\n- Clique em 'Sim' para UPGRADE.$\n- Clique em 'Cancelar' para DESINSTALAR.$\n- Clique em 'Não' para Cancelar."

LangString MsgDowngrade 1033 "NEWER version ($0) is already installed!$\n$\nDo you want to DOWNGRADE to an older version (${VERSION})?$\n$\n- Click 'Yes' to DOWNGRADE.$\n- Click 'Cancel' to UNINSTALL.$\n- Click 'No' to Cancel."
LangString MsgDowngrade 1057 "WhatsZan versi LEBIH BARU ($0) terdeteksi!$\n$\nApakah kamu ingin melakukan DOWNGRADE ke versi lama (${VERSION})?$\n$\n- Klik 'Yes' untuk DOWNGRADE.$\n- Klik 'Cancel' untuk UNINSTALL.$\n- Klik 'No' untuk Batal."
LangString MsgDowngrade 1031 "NEUERE Version ($0) ist bereits installiert!$\n$\nMöchten Sie auf eine ältere Version (${VERSION}) DOWNGRADEN?$\n$\n- Klicken Sie 'Ja' zum DOWNGRADE.$\n- Klicken Sie 'Abbrechen' zum DEINSTALLIEREN.$\n- Klicken Sie 'Nein' zum Abbrechen."
LangString MsgDowngrade 1036 "Une version PLUS RÉCENTE ($0) est déjà installée !$\n$\nVoulez-vous rétrograder vers une ancienne version (${VERSION}) ?$\n$\n- Cliquez 'Oui' pour RETROGRADER.$\n- Cliquez 'Annuler' pour DÉSINSTALLER.$\n- Cliquez 'Non' pour Annuler."
LangString MsgDowngrade 1040 "È già installata una versione PIÙ RECENTE ($0)!$\n$\nVuoi fare il DOWNGRADE a una versione precedente (${VERSION})?$\n$\n- Clicca 'Sì' per DOWNGRADE.$\n- Clicca 'Annulla' per DISINSTALLARE.$\n- Clicca 'No' per Annullare."
LangString MsgDowngrade 1046 "Uma versão MAIS RECENTE ($0) já está instalada!$\n$\nDeseja fazer o DOWNGRADE para uma versão antiga (${VERSION})?$\n$\n- Clique em 'Sim' para DOWNGRADE.$\n- Clique em 'Cancelar' para DESINSTALAR.$\n- Clique em 'Não' para Cancelar."

LangString MsgRepair 1033 "WhatsZan version ${VERSION} is already installed.$\n$\nDo you want to REPAIR the application?$\n$\n- Click 'Yes' to REPAIR.$\n- Click 'Cancel' to UNINSTALL.$\n- Click 'No' to Cancel."
LangString MsgRepair 1057 "WhatsZan versi ${VERSION} sudah terinstal.$\n$\nApakah kamu ingin melakukan REPAIR (Perbaikan) aplikasi?$\n$\n- Klik 'Yes' untuk REPAIR.$\n- Klik 'Cancel' untuk UNINSTALL.$\n- Klik 'No' untuk Batal."
LangString MsgRepair 1031 "WhatsZan Version ${VERSION} ist bereits installiert.$\n$\nMöchten Sie die Anwendung REPARIEREN?$\n$\n- Klicken Sie 'Ja' zum REPARIEREN.$\n- Klicken Sie 'Abbrechen' zum DEINSTALLIEREN.$\n- Klicken Sie 'Nein' zum Abbrechen."
LangString MsgRepair 1036 "WhatsZan version ${VERSION} est déjà installée.$\n$\nVoulez-vous REPARER l'application ?$\n$\n- Cliquez 'Oui' pour REPARER.$\n- Cliquez 'Annuler' pour DÉSINSTALLER.$\n- Cliquez 'Non' pour Annuler."
LangString MsgRepair 1040 "WhatsZan versione ${VERSION} è già installata.$\n$\nVuoi RIPARARE l'applicazione?$\n$\n- Clicca 'Sì' per RIPARARE.$\n- Clicca 'Annulla' per DISINSTALLARE.$\n- Clicca 'No' per Annullare."
LangString MsgRepair 1046 "WhatsZan versão ${VERSION} já está instalada.$\n$\nDeseja REPARAR o aplicativo?$\n$\n- Clique em 'Sim' para REPARAR.$\n- Clique em 'Cancelar' para DESINSTALAR.$\n- Clique em 'Não' para Cancelar."


!macro customInit
  ; Read the currently installed version (from HKLM or HKCU depending on installation scope)
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "DisplayVersion"
  ${If} $0 == ""
    ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "DisplayVersion"
  ${EndIf}

  ${If} $0 != ""
    ${VersionCompare} "${VERSION}" $0 $1
    ; $1 = 0 (Same), 1 (Newer), 2 (Older)
    
    ${If} $1 == 1
      ; Installer is newer (Upgrade)
      MessageBox MB_YESNOCANCEL|MB_ICONINFORMATION \
        $(MsgUpgrade) \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${ElseIf} $1 == 2
      ; Installer is older (Downgrade)
      MessageBox MB_YESNOCANCEL|MB_ICONEXCLAMATION \
        $(MsgDowngrade) \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${Else}
      ; Same version (Re-install / Repair)
      MessageBox MB_YESNOCANCEL|MB_ICONINFORMATION \
        $(MsgRepair) \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${EndIf}

    AbortInstall:
      Quit

    DoUninstall:
      ; Silently force-close the running application before uninstall
      nsExec::Exec 'taskkill /F /IM whatszan.exe /T'
      nsExec::Exec 'taskkill /F /IM WhatsZan.exe /T'
      
      ReadRegStr $2 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "UninstallString"
      ${If} $2 == ""
        ReadRegStr $2 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "UninstallString"
      ${EndIf}
      
      ${If} $2 != ""
        ; Run the uninstaller without silent mode (/S) so the UI and data deletion prompt appear
        ExecWait '"$2"'
      ${EndIf}
      Quit

    ContinueInstall:
      ; Silently force-close the running application before update/downgrade/repair
      nsExec::Exec 'taskkill /F /IM whatszan.exe /T'
      nsExec::Exec 'taskkill /F /IM WhatsZan.exe /T'
      ; Continue with normal installation process
  ${EndIf}
!macroend

; ── After Installation Completes ─────────────────────────────────────────
; Silently creates a shortcut in the Windows Startup folder.
; The user won't be bothered by a popup, but the app will auto-start.
!macro customInstall
  ; $SMSTARTUP = C:\Users\[User]\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup
  CreateShortCut \
    "$SMSTARTUP\WhatsZan.lnk" \
    "$INSTDIR\WhatsZan.exe" \
    "--hide" \
    "$INSTDIR\WhatsZan.exe" 0
    
  ; Create a flag file so the app knows it was just installed/repaired and needs to re-apply custom shortcuts
  FileOpen $9 "$APPDATA\WhatsZan\reapply-shortcuts.flag" w
  FileWrite $9 "1"
  FileClose $9
!macroend

; ── During Uninstallation ────────────────────────────────────────────────
; Remove shortcut from the Startup folder to leave no leftovers.
!macro customUnInstall
  Delete "$SMSTARTUP\WhatsZan.lnk"
  Delete "$DESKTOP\WhatsApp.lnk"
  Delete "$SMPROGRAMS\WhatsApp.lnk"
  
  ; Write and execute a powershell script to dynamically delete renamed shortcuts
  FileOpen $9 "$TEMP\clean_whatszan_shortcuts.ps1" w
  FileWrite $9 "$$p = '$APPDATA\WhatsZan\persistent-state.json'$\r\n"
  FileWrite $9 "if (Test-Path $$p) {$\r\n"
  FileWrite $9 "  $$c = Get-Content $$p -Raw | ConvertFrom-Json$\r\n"
  FileWrite $9 "  $$n = $$c.'custom-app-name'$\r\n"
  FileWrite $9 "  if ($$n) {$\r\n"
  FileWrite $9 "    Remove-Item -Path '$DESKTOP\$$n.lnk' -ErrorAction SilentlyContinue$\r\n"
  FileWrite $9 "    Remove-Item -Path '$SMPROGRAMS\$$n.lnk' -ErrorAction SilentlyContinue$\r\n"
  FileWrite $9 "  }$\r\n"
  FileWrite $9 "}$\r\n"
  FileClose $9
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$TEMP\clean_whatszan_shortcuts.ps1"'
  Delete "$TEMP\clean_whatszan_shortcuts.ps1"
  
  IfSilent keep_data
  MessageBox MB_YESNO|MB_ICONQUESTION $(MsgUninstData) IDNO keep_data
    RMDir /r "$APPDATA\whatszan"
    RMDir /r "$LOCALAPPDATA\whatszan-updater"
  keep_data:
!macroend

; ── When the Uninstaller Starts ──────────────────────────────────────────
; Called when the user uninstalls from Windows Control Panel
!macro customUnInit
  ; Silently force-close the running application before the uninstall process
  nsExec::Exec 'taskkill /F /IM whatszan.exe /T'
  nsExec::Exec 'taskkill /F /IM WhatsZan.exe /T'
!macroend
