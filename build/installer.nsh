; installer.nsh — Custom NSIS hooks untuk WhatsZan
; Ditambahkan oleh electron-builder via opsi "include"

!include "WordFunc.nsh"

!macro customInit
  ; Baca versi yang sudah terinstal (bisa di HKLM atau HKCU tergantung instalasi)
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "DisplayVersion"
  ${If} $0 == ""
    ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "DisplayVersion"
  ${EndIf}

  ${If} $0 != ""
    ${VersionCompare} "${VERSION}" $0 $1
    ; $1 = 0 (Sama), 1 (Lebih Baru), 2 (Lebih Lama)
    
    ${If} $1 == 1
      ; Installer lebih baru (Update)
      MessageBox MB_YESNOCANCEL|MB_ICONINFORMATION \
        "WhatsZan versi lama ($0) sudah ada di komputermu.$\n$\nApakah kamu ingin melakukan UPDATE ke versi terbaru (${VERSION})?$\n$\n- Pilih 'Yes' untuk Update.$\n- Pilih 'No' untuk membatalkan instalasi.$\n- Pilih 'Cancel' untuk UNINSTALL versi lama." \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${ElseIf} $1 == 2
      ; Installer lebih lama (Downgrade)
      MessageBox MB_YESNOCANCEL|MB_ICONEXCLAMATION \
        "WhatsZan versi LEBIH BARU ($0) sudah terinstal!$\n$\nApakah kamu yakin ingin melakukan DOWNGRADE ke versi lama (${VERSION})?$\n$\n- Pilih 'Yes' untuk Downgrade.$\n- Pilih 'No' untuk membatalkan instalasi.$\n- Pilih 'Cancel' untuk UNINSTALL aplikasi." \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${Else}
      ; Versi sama (Re-install)
      MessageBox MB_YESNOCANCEL|MB_ICONINFORMATION \
        "WhatsZan versi ${VERSION} sudah terinstal saat ini.$\n$\nApakah kamu ingin melakukan INSTAL ULANG?$\n$\n- Pilih 'Yes' untuk Instal Ulang.$\n- Pilih 'No' untuk membatalkan instalasi.$\n- Pilih 'Cancel' untuk UNINSTALL aplikasi." \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${EndIf}

    AbortInstall:
      Quit

    DoUninstall:
      ReadRegStr $2 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "UninstallString"
      ${If} $2 == ""
        ReadRegStr $2 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "UninstallString"
      ${EndIf}
      
      ${If} $2 != ""
        ExecWait '"$2" /S'
      ${EndIf}
      Quit

    ContinueInstall:
      ; Melanjutkan ke proses instalasi normal
  ${EndIf}
!macroend

; ── Setelah Instalasi Selesai ──────────────────────────────────────────────
; Menampilkan dialog tanya apakah user ingin WhatsZan auto-start saat login.
; Jika Ya: buat shortcut .lnk di folder Startup Windows agar terlihat di
;          Task Manager → tab Startup.
; Jika Tidak: tidak ada perubahan.
!macro customInstall
  ; Tanya user apakah mau auto-start
  MessageBox MB_ICONQUESTION|MB_YESNO \
    "Apakah kamu ingin WhatsZan berjalan otomatis saat Windows dinyalakan?$\n$\nJika Ya, WhatsZan akan langsung berjalan di background (system tray) saat login." \
    IDNO SkipAutorun

    ; Buat shortcut di folder Startup Windows
    ; $SMSTARTUP = C:\Users\[User]\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup
    CreateShortCut \
      "$SMSTARTUP\WhatsZan.lnk" \
      "$INSTDIR\WhatsZan.exe" \
      "--hide" \
      "$INSTDIR\WhatsZan.exe" 0

  SkipAutorun:
!macroend

; ── Saat Uninstalasi ───────────────────────────────────────────────────────
; Hapus shortcut dari folder Startup agar tidak ada sisa.
!macro customUnInstall
  Delete "$SMSTARTUP\WhatsZan.lnk"
!macroend
