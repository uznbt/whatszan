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
        "WhatsZan versi lama ($0) terdeteksi.$\n$\nApakah kamu ingin melakukan UPGRADE ke versi terbaru (${VERSION})?$\n$\n- Klik 'Yes' untuk UPGRADE.$\n- Klik 'Cancel' untuk UNINSTALL.$\n- Klik 'No' untuk Batal." \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${ElseIf} $1 == 2
      ; Installer lebih lama (Downgrade)
      MessageBox MB_YESNOCANCEL|MB_ICONEXCLAMATION \
        "WhatsZan versi LEBIH BARU ($0) terdeteksi!$\n$\nApakah kamu ingin melakukan DOWNGRADE ke versi lama (${VERSION})?$\n$\n- Klik 'Yes' untuk DOWNGRADE.$\n- Klik 'Cancel' untuk UNINSTALL.$\n- Klik 'No' untuk Batal." \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${Else}
      ; Versi sama (Re-install)
      MessageBox MB_YESNOCANCEL|MB_ICONINFORMATION \
        "WhatsZan versi ${VERSION} sudah terinstal.$\n$\nApakah kamu ingin melakukan REPAIR (Perbaikan) aplikasi?$\n$\n- Klik 'Yes' untuk REPAIR.$\n- Klik 'Cancel' untuk UNINSTALL.$\n- Klik 'No' untuk Batal." \
        IDYES ContinueInstall IDNO AbortInstall
      Goto DoUninstall
    ${EndIf}

    AbortInstall:
      Quit

    DoUninstall:
      ; Matikan paksa aplikasi yang sedang berjalan secara diam-diam sebelum uninstall
      nsExec::Exec 'taskkill /F /IM whatszan.exe /T'
      nsExec::Exec 'taskkill /F /IM WhatsZan.exe /T'
      
      ReadRegStr $2 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "UninstallString"
      ${If} $2 == ""
        ReadRegStr $2 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_APP_KEY}" "UninstallString"
      ${EndIf}
      
      ${If} $2 != ""
        ; Jalankan uninstaller tanpa mode silent (/S) agar UI dan pertanyaan hapus data muncul
        ExecWait '"$2"'
      ${EndIf}
      Quit

    ContinueInstall:
      ; Matikan paksa aplikasi yang sedang berjalan secara diam-diam sebelum update/downgrade/repair
      nsExec::Exec 'taskkill /F /IM whatszan.exe /T'
      nsExec::Exec 'taskkill /F /IM WhatsZan.exe /T'
      ; Melanjutkan ke proses instalasi normal
  ${EndIf}
!macroend

; ── Setelah Instalasi Selesai ──────────────────────────────────────────────
; Secara diam-diam (silent) membuat shortcut di folder Startup Windows.
; User tidak akan diganggu oleh popup, tapi aplikasi akan otomatis auto-start.
!macro customInstall
  ; $SMSTARTUP = C:\Users\[User]\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup
  CreateShortCut \
    "$SMSTARTUP\WhatsZan.lnk" \
    "$INSTDIR\WhatsZan.exe" \
    "--hide" \
    "$INSTDIR\WhatsZan.exe" 0
!macroend

; ── Saat Uninstalasi ───────────────────────────────────────────────────────
; Hapus shortcut dari folder Startup agar tidak ada sisa.
!macro customUnInstall
  Delete "$SMSTARTUP\WhatsZan.lnk"
!macroend

; ── Saat Uninstaller Dimulai ───────────────────────────────────────────────
; Ini akan dipanggil ketika user melakukan uninstall dari Control Panel Windows
!macro customUnInit
  ; Matikan paksa aplikasi yang sedang berjalan secara diam-diam sebelum proses uninstall
  nsExec::Exec 'taskkill /F /IM whatszan.exe /T'
  nsExec::Exec 'taskkill /F /IM WhatsZan.exe /T'
!macroend
