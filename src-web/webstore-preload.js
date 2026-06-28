const { contextBridge, ipcRenderer } = require('electron');

// Expose a safe API to the injected script if needed, but we can do everything in DOM directly
contextBridge.exposeInMainWorld('whatszanExt', {
  installExtension: (id) => ipcRenderer.invoke('install-extension', id)
});

// Gunakan setInterval agar 100% dipanggil meskipun halaman ganti via SPA (Single Page Application)
setInterval(() => {
  try {
    // Hanya injeksi di halaman detail ekstensi
    if (!window.location.pathname.startsWith('/detail/')) {
      // Jika user pindah ke halaman lain, sembunyikan tombol jika ada
      const btn = document.getElementById('whatszan-install-btn');
      if (btn) btn.style.display = 'none';
      return;
    }

    // Ekstrak ID ekstensi (32 karakter a-p) dari URL
    const match = window.location.pathname.match(/([a-p]{32})/);
    if (!match) return;
    const extensionId = match[1];

    // Cek apakah tombol sudah ada
    let btnContainer = document.getElementById('whatszan-install-btn');
    if (!btnContainer) {
      injectInstallButton(extensionId);
    } else {
      // Pastikan tombol muncul kembali (jika sebelumnya di-hide)
      btnContainer.style.display = 'flex';
      // Pastikan ID-nya update jika user pindah ke ekstensi lain di tab yg sama
      btnContainer.dataset.extId = extensionId;
    }
  } catch (e) {}
}, 1000);

function injectInstallButton(extensionId) {
  if (!document.body) return;

  const btnContainer = document.createElement('div');
  btnContainer.id = 'whatszan-install-btn';
  btnContainer.dataset.extId = extensionId;
  btnContainer.style.position = 'fixed';
  btnContainer.style.top = '100px'; // Turunkan sedikit menghindari header Chrome Store
  btnContainer.style.right = '30px';
  btnContainer.style.zIndex = '999999999';
  btnContainer.style.background = '#00a884';
  btnContainer.style.color = '#fff';
  btnContainer.style.padding = '12px 24px';
  btnContainer.style.borderRadius = '24px';
  btnContainer.style.fontFamily = 'Segoe UI, Helvetica, Arial, sans-serif';
  btnContainer.style.fontWeight = 'bold';
  btnContainer.style.cursor = 'pointer';
  btnContainer.style.boxShadow = '0 8px 24px rgba(0,0,0,0.4)';
  btnContainer.style.display = 'flex';
  btnContainer.style.alignItems = 'center';
  btnContainer.style.gap = '8px';
  btnContainer.style.transition = 'background 0.2s, transform 0.2s';
  
  btnContainer.innerHTML = `
    <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
      <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z"/>
    </svg>
    Pasang di WhatsZan
  `;

  btnContainer.onmouseover = () => {
    btnContainer.style.background = '#06cf9c';
    btnContainer.style.transform = 'scale(1.05)';
  };
  btnContainer.onmouseleave = () => {
    btnContainer.style.background = '#00a884';
    btnContainer.style.transform = 'scale(1)';
  };

  btnContainer.onclick = async () => {
    const targetId = btnContainer.dataset.extId;
    btnContainer.innerHTML = 'Menginstal...';
    btnContainer.style.background = '#8696a0';
    btnContainer.style.pointerEvents = 'none';
    
    try {
      const success = await ipcRenderer.invoke('install-extension', targetId);
      if (btnContainer) {
        if (success) {
          btnContainer.innerHTML = window.__wzWebstoreInstalled || 'Berhasil Terpasang';
          btnContainer.style.background = '#00a884';
        } else {
          btnContainer.innerHTML = window.__wzWebstoreFailed || 'Gagal Menginstal';
          btnContainer.style.background = '#ea0038';
        }
      }
    } catch (err) {
      btnContainer.innerHTML = 'Error';
      btnContainer.style.background = '#ea0038';
    }
    
    setTimeout(() => {
      btnContainer.style.display = 'none';
      btnContainer.style.pointerEvents = 'auto'; // Reset for future visits
    }, 5000);
  };

  // Tempelkan ke document.documentElement (tag HTML) alih-alih body
  // karena SPA seringkali me-reset isi <body>
  document.documentElement.appendChild(btnContainer);
}
