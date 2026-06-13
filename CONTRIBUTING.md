# Panduan Kontribusi (Contributing Guidelines)

Terima kasih atas ketertarikanmu untuk berkontribusi di **WhatsZan**! Kontribusi dalam bentuk apa pun sangat diterima dan dihargai.

## Cara Berkontribusi

Jika kamu menemukan *bug*, memiliki ide fitur baru, atau ingin memperbaiki terjemahan bahasa, silakan ikuti langkah-langkah berikut:

### 1. Melaporkan Bug atau Meminta Fitur Baru (Issues)
Sebelum mulai menulis kode, ada baiknya kamu membuka sebuah [Issue](https://github.com/uznbt/whatszan/issues) baru untuk mendiskusikan apa yang ingin kamu perbaiki atau tambahkan. Ini memastikan bahwa upaya yang kamu keluarkan sejalan dengan arah proyek ini.

### 2. Mengirimkan Kode Perbaikan (Pull Requests)
Jika kamu sudah siap untuk menulis kode:
1. **Fork** repositori ini ke akun GitHub kamu.
2. Lakukan *Clone* ke komputer lokalmu:
   ```bash
   git clone https://github.com/<username-kamu>/whatszan.git
   ```
3. Buat *branch* baru untuk fitur atau perbaikanmu:
   ```bash
   git checkout -b fitur-baru-kamu
   ```
4. Lakukan perubahan pada kode. Pastikan kamu menjalankan `npm start` untuk mengetes perubahanmu secara lokal.
5. Lakukan *Commit* perubahanmu dengan pesan yang jelas dan deskriptif:
   ```bash
   git commit -m "feat: tambahkan fitur keren"
   ```
6. *Push* ke *branch* di repositori *fork* milikmu:
   ```bash
   git push origin fitur-baru-kamu
   ```
7. Terakhir, buka halaman repositori utama dan buat sebuah **Pull Request (PR)** baru.

Semua perbaikan kode dan diskusi sangat dihargai agar klien WhatsApp ini tetap ringan dan stabil untuk semua pengguna Linux. Mari berkolaborasi!
