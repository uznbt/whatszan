# WhatsZan - Unofficial WhatsApp Desktop Client untuk Linux

<p align="center">
<img width="400" alt="whatszan" src="assets/images.jpg" />
</p>

## Fitur

* **[BARU] Jendela Pengaturan Terpusat**: Mengatur seluruh aplikasi dari satu jendela (`Ctrl + ,`) dengan fitur **Simpan Otomatis** (Auto-Save).
* **[BARU] Mode Rahasia Lanjutan (Privacy Blur)**: Sensor (blur) layar secara otomatis saat ada orang lewat. Dilengkapi pengaturan spesifik untuk blur Foto Profil, Pesan, Media, dan **bahkan hanya untuk kontak tertentu saja**!
* **[BARU] Integrasi Ekstensi Chrome**: Pasang berbagai ekstensi secara langsung dari Chrome Web Store lewat Jendela Pengaturan.
* **[BARU] Personalisasi UI**: Merubah logo dan tulisan jendela antarmuka web WhatsApp menjadi identitas "WhatsZan" secara mulus.
* **Deteksi Tema Sistem Universal**: Secara akurat mendeteksi dan mensinkronisasi tema Gelap/Terang (*Dark/Light Mode*) secara *real-time* di berbagai Desktop Environment Linux (bekerja mulus di GNOME/GTK, maupun KDE Plasma/Hyprland berbasis Qt).
* **Jalankan Otomatis & Minimalkan**: Menyala otomatis di latar belakang saat komputer dihidupkan (Dapat diatur melalui Menu Jendela/Window).
* **Routing Link Mulus (Anti-Reload)**: Menangani tautan `chat.whatsapp.com`, `wa.me`, dan `whatsapp://` secara *native* tanpa memuat ulang halaman (*reload*) atau menyebabkan layar putih.
* **Anti Tracker Bypass**: Otomatis membongkar pelacak tautan internal WhatsApp (`l.wl.co`) sehingga mencegah browser eksternal terbuka secara tidak sengaja.
* **Dukungan Multi-Bahasa**: Menu diterjemahkan sepenuhnya ke dalam bahasa Indonesia, Inggris, Jerman, Prancis, Italia, dan Portugis.
* Notifikasi desktop
* Panggilan Telepon / Video berfungsi (kamu harus bergabung dengan WhatsApp Web Beta)
* Ikon tray dengan jumlah pesan belum dibaca (AppIndicator), [ikon tray kustom](#ikon-kustom).
* Pintasan keyboard kustom
* CSS Kustom & Pemeriksa Ejaan (*Spellcheck*)
* Antarmuka CLI & D-Bus untuk memunculkan/menyembunyikan jendela
* Lencana jumlah pesan belum dibaca pada dock & Menyimpan posisi jendela terakhir
* Skrip pengguna kustom di `~/.config/whatszan/user-scripts/*.js`

## Mengapa?

Saya membuat *fork* dari [elecwhat](https://github.com/piec/elecwhat) ini untuk memperbaiki masalah kestabilan pada cara penanganan tautan (`wa.me` / `chat.whatsapp.com`), mencegah jendela browser eksternal terbuka secara tak terduga, dan menambahkan fitur latar belakang yang penting seperti "Jalankan Otomatis & Minimalkan". Aplikasi ini tetap mempertahankan sifatnya yang sederhana, ringan, dan stabil seperti klien aslinya.

## Instalasi

### Ubuntu/Kubuntu:
* [Snap](#)
* **atau** file `.deb` di tab [Releases] dengan `--no-sandbox` karena ada [isu Electron]
* **atau** file `AppImage` di tab [Releases] dengan `--no-sandbox` karena ada [isu Electron]

### Arch Linux:
* Paket AUR: `yay -S whatszan-bin` *(jika tersedia)*
* **atau** paket Pacman di tab [Releases]
* **atau** file `AppImage` di tab [Releases]

### Debian:
* file `.deb` di tab [Releases]
* **atau** file `AppImage` di tab [Releases]

### Fedora:
* file `.rpm` di tab [Releases]
* **atau** file `AppImage` di tab [Releases]

---

**Instalasi Manual (dari Source Code):**

```bash
# Clone repositori
git clone https://github.com/username-kamu/whatszan-electron.git
cd whatszan-electron

# Instal dependensi
npm install

# Jalankan aplikasi
npm start

# Build aplikasi (hasilnya akan ada di folder /dist atau /out)
npm run build
```

## Konfigurasi

`~/.config/whatszan/config.json`:

```json
{
  "log-level": "info",
  "notification-prefix": "whatszan - ",
  "quit-on-close": false,
  "show-at-startup": false,
  "dbus": true,
  "menu-bar": true,
  "menu-bar-auto-hide": true,
  "keys": {
    "C ArrowDown": {
      "whatsappAction": "GO_TO_NEXT_CHAT"
    }
  },
  "esc-toggle-window": false,
  "css": [],
  "spellcheck": true,
  "spellcheck-languages": ["id", "en-US", "fr"],
  "low-resource-mode": true,
  "user-agent": "...",
  "open-dev-tools": false,
  "scripts": [],
  "icons-directory": "..."
}
```

### Mode Sumber Daya Rendah
`"low-resource-mode": true` (default) mematikan layanan-layanan latar belakang
Chromium yang tidak pernah dipakai WhatsApp Web: Google Translate, Cast /
MediaRouter, sinkronisasi, deteksi phishing, domain reliability, dan
preconnect/prefetch di latar belakang. Tidak ada satu pun dari fitur itu yang
dipakai WhatsApp Web, jadi functionality tetap utuh.

Perubahan ini **berlaku setelah aplikasi dimulai ulang** (flag Chromium hanya
bisa dibaca saat startup). Buka **Pengaturan → Tampilan** untuk menyalakan /
mematikannya.

### Pintasan Keyboard di Linux / Wayland

`Ctrl+Shift+B` (Mode Rahasia) dan `Ctrl+Shift+H` (Anti Screen Cast) adalah
*global shortcut*. Chromium hanya mengimplementasikan global shortcut lewat X11
(XGrabKey).

Di **Wayland** (Hyprland, Sway, KDE Wayland, GNOME Wayland) `globalShortcut`
dipaksa bekerja oleh XWayland sehingga tidak pernah meneruskan event ke
aplikasi. Chromium membuat `globalShortcut.register()` tetap terlihat sukses
dan `isRegistered()` juga `true`, padahal pintasan tersebut diam-diam tidak
berfungsi.

Karena itu WhatsZan mendeteksi sesi Wayland lalu memproses kedua pintasan itu
di sisi renderer. Konsekuensinya:

- Pintasan tetap bekerja **selagi jendela WhatsApp aktif** — persis skenario
  pemakaian fitur ini (menahan/menekan tombol saat sedang melihat WhatsApp).
- Di X11 dan Windows, pintasan tetap bersifat global (bisa ditekan dari
  jendela lain) seperti sebelumnya.

### Fitur Incognito / Power User

* **Ghost Mode: Centang Biru** — lawan bicara tidak tahu pesannya sudah dibaca.
* **Ghost Mode: Sembunyikan Mengetik** — sembunyikan indikator sedang mengetik.
* **Anti-Hapus Pesan** — pulihkan pesan yang ditarik pengirim.
* **Pengunduh Status (Status Saver)** — tombol "Simpan Status" muncul saat
  melihat Story, dan status bisa diunduh ke folder gambar.

## Ikon Kustom

Skema penamaan ikon didasarkan pada URL favicon WhatsApp Web:

* `~/.config/whatszan/user-icons/app.png` saat startup
* `~/.config/whatszan/user-icons/favicon.png` ketika tidak ada pesan belum dibaca
* `~/.config/whatszan/user-icons/f01.png` di mana fXX.png adalah jumlah pesan yang belum dibaca

## Kredit & Ucapan Terima Kasih

Proyek ini adalah *fork* yang dimodifikasi secara ekstensif dari **[elecwhat](https://github.com/piec/elecwhat)**.
Terima kasih yang sebesar-besarnya kepada kreator asli, [piec](https://github.com/piec), yang telah membangun fondasi Electron yang sangat stabil, integrasi DBus, dan struktur *preload script* yang sangat rapi.

Modifikasi pada repositori ini (seperti fitur *Auto-Run*, routing tautan internal yang mulus, pemblokiran *tracker*, dan terjemahan bahasa Indonesia) dikembangkan secara spesifik untuk aplikasi WhatsZan dengan bantuan *AI coding assistant*.
